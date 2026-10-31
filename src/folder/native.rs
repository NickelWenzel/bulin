//! A folder on disk: reads, writes through a temp file and a rename, a watcher, and the system's
//! dialogs to pick a folder or a project.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, Stream, StreamExt};
use iced::{Element, Task};
use notify_debouncer_full::notify::{self, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};

use super::opener::Instruction;
use super::{Error, Watched};
use crate::action::Action;
use crate::core::folder::{
    self, Change, Entry, Expect, Kind, MAX_TEXT, Path, SKIP, TMP, reads_text,
};
use crate::core::library;
use crate::file;

/// How long the watcher waits for a file's events to settle before reporting it.
const DEBOUNCE: Duration = Duration::from_millis(150);

/// Numbers temp files, so writes running at once never share one.
static TEMP: AtomicU64 = AtomicU64::new(0);

/// A folder, by the directory it is.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Handle {
    root: PathBuf,
}

/// Picks folders and projects with the system's dialogs.
#[derive(Debug, Default)]
pub struct Opener {
    picking: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    FolderPicked(Option<PathBuf>),
    ProjectPicked(Result<(file::Name, Arc<String>), file::Error>),
}

type Watcher = Debouncer<RecommendedWatcher, RecommendedCache>;

impl Handle {
    /// The folder that is the directory `root`.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// The directory, where the language server analyzes the folder.
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    /// The directory's name.
    pub fn name(&self) -> &str {
        self.root
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("folder")
    }

    /// Nothing: a directory isn't in the browser's library.
    pub fn library_id(&self) -> Option<library::Id> {
        None
    }
}

impl Opener {
    /// Asks the user for a folder.
    pub fn open_folder(&mut self) -> Task<Message> {
        self.picking = true;
        Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .set_title("Open a folder...")
                    .pick_folder()
                    .await
                    .map(|folder| folder.path().to_owned())
            },
            Message::FolderPicked,
        )
    }

    /// Asks the user for a project file.
    pub fn open_project(&mut self) -> Task<Message> {
        self.picking = true;
        Task::perform(file::open_file(), Message::ProjectPicked)
    }

    pub fn update(&mut self, message: Message) -> Action<Instruction, Message> {
        self.picking = false;
        match message {
            Message::FolderPicked(Some(root)) => {
                Action::instruction(Instruction::OpenFolder(Handle::new(root)))
            }
            Message::FolderPicked(None)
            | Message::ProjectPicked(Err(file::Error::DialogClosed)) => Action::none(),
            Message::ProjectPicked(Ok((name, text))) => {
                Action::instruction(Instruction::OpenProject(name, text))
            }
            Message::ProjectPicked(Err(error)) => Action::instruction(Instruction::Failed(
                format!("Could not open the project: {error}"),
            )),
        }
    }

    /// Nothing: the system's dialogs show themselves, and no library entry is open.
    pub fn view(&self, _current: Option<library::Id>) -> Option<Element<'_, Message>> {
        None
    }

    /// Nothing: a system dialog can't be closed from outside.
    pub fn close(&mut self) {}

    /// Whether a dialog is open.
    pub fn is_busy(&self) -> bool {
        self.picking
    }
}

/// The text of the file `path` in the folder at `handle`.
pub async fn read(handle: Handle, path: Path) -> Result<String, Error> {
    let bytes = tokio::fs::read(handle.root.join(path.as_str()))
        .await
        .map_err(error)?;
    if bytes.len() > MAX_TEXT {
        return Err(Error::Io(format!("{path} is larger than 1 MiB")));
    }
    String::from_utf8(bytes).map_err(|_| Error::Io(format!("{path} is not UTF-8 text")))
}

/// The entries of the directory `dir` in the folder at `handle`, `None` being the folder itself,
/// sorted with directories first. Names that aren't valid paths are left out, and a symlink to a
/// directory lists as a directory.
pub async fn scan_dir(handle: Handle, dir: Option<Path>) -> Result<Vec<Entry>, Error> {
    let directory = match &dir {
        Some(dir) => handle.root.join(dir.as_str()),
        None => handle.root,
    };
    let mut listing = tokio::fs::read_dir(directory).await.map_err(error)?;
    let mut entries = Vec::new();

    while let Some(entry) = listing.next_entry().await.map_err(error)? {
        let Some(path) = entry
            .file_name()
            .to_str()
            .and_then(|name| Path::in_dir(dir.as_ref(), name).ok())
        else {
            continue;
        };
        let kind = match tokio::fs::metadata(entry.path()).await {
            Ok(metadata) if metadata.is_dir() => Kind::Dir,
            Ok(_) | Err(_) => Kind::File,
        };
        entries.push(Entry { kind, path });
    }
    entries.sort();
    Ok(entries)
}

/// Writes `text` to the file `path` in the folder at `handle`, if the file is what `expect` says.
///
/// The text goes to a temp file under `.bulin/tmp/` first, which then replaces the file, so the
/// file is never half written. The file keeps its permissions. A symlink is written through
/// instead, so it stays a link; a hard link is replaced by a file of its own.
pub async fn write(
    handle: Handle,
    path: Path,
    text: String,
    expect: Expect,
) -> Result<Vec<Change>, Error> {
    let target = handle.root.join(path.as_str());
    let temps = handle.root.join(TMP);
    let written = text.clone();

    tokio::task::spawn_blocking(move || {
        let disk = match fs::read(&target) {
            Ok(bytes) => Some(String::from_utf8_lossy(&bytes).into_owned()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(self::error(error)),
        };
        if !expect.matches(disk.as_deref()) {
            return Err(Error::Conflict { disk });
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(self::error)?;
        }

        let is_link = fs::symlink_metadata(&target).is_ok_and(|metadata| metadata.is_symlink());
        if is_link || fs::create_dir_all(&temps).is_err() {
            return fs::write(&target, &written).map_err(self::error);
        }
        let temp = temps.join(format!(
            "{}-{}.tmp",
            std::process::id(),
            TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| -> io::Result<()> {
            let mut file = fs::File::create(&temp)?;
            io::Write::write_all(&mut file, written.as_bytes())?;
            file.sync_all()?;
            if let Ok(metadata) = fs::metadata(&target) {
                fs::set_permissions(&temp, metadata.permissions())?;
            }
            match fs::rename(&temp, &target) {
                // A directory on another file system, which a rename can't reach.
                Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
                    fs::write(&target, &written)
                }
                result => result,
            }
        })();
        // Gone after a rename; left behind by a failure or the in-place fallback.
        let _ = fs::remove_file(&temp);
        result.map_err(self::error)
    })
    .await
    .map_err(|error| Error::Io(error.to_string()))??;

    Ok(vec![Change::Changed {
        text: reads_text(&path).then_some(text),
        path,
    }])
}

/// Creates the directory `path` in the folder at `handle`, refusing one that exists.
pub async fn create_dir(handle: Handle, path: Path) -> Result<(), Error> {
    tokio::fs::create_dir(handle.root.join(path.as_str()))
        .await
        .map_err(|error| match error.kind() {
            io::ErrorKind::AlreadyExists => Error::Io(format!("{path} already exists")),
            _ => self::error(error),
        })
}

/// Moves the file or directory `from` in the folder at `handle` to `to`, refusing a `to` that
/// exists. Reports `from` removed and each file now under `to` changed, with its text as the
/// watcher reads it.
pub async fn rename(handle: Handle, from: Path, to: Path) -> Result<Vec<Change>, Error> {
    let root = handle.root;
    tokio::task::spawn_blocking(move || {
        let source = root.join(from.as_str());
        let target = root.join(to.as_str());
        if fs::symlink_metadata(&target).is_ok() {
            return Err(Error::Io(format!("{to} already exists")));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(error)?;
        }
        fs::rename(&source, &target).map_err(error)?;

        let mut changes = vec![Change::Removed(from)];
        match fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.is_dir() => changes.extend(
                walk(&target)
                    .1
                    .into_iter()
                    .filter_map(|file| Some(changed(relative(&root, &file)?, &file))),
            ),
            Ok(_) | Err(_) => changes.push(changed(to, &target)),
        }
        Ok(changes)
    })
    .await
    .map_err(|error| Error::Io(error.to_string()))?
}

/// Deletes the file or directory `path` in the folder at `handle`, a directory with everything in
/// it. A symlink is removed, not what it points to.
pub async fn remove(handle: Handle, path: Path) -> Result<Vec<Change>, Error> {
    let target = handle.root.join(path.as_str());
    let metadata = tokio::fs::symlink_metadata(&target).await.map_err(error)?;
    let removed = if metadata.is_dir() {
        tokio::fs::remove_dir_all(&target).await
    } else {
        tokio::fs::remove_file(&target).await
    };
    removed.map_err(error)?;
    Ok(vec![Change::Removed(path)])
}

/// Watches the folder at `handle`: first a scan of its files, then what changes on disk.
///
/// Leftover temp files are removed first, and the scan follows registering the watches, so no
/// change falls between the two.
pub fn stream(handle: Handle) -> impl Stream<Item = Watched> + Send {
    iced::stream::channel(16, async move |mut output: mpsc::Sender<Watched>| {
        let (sender, mut events) = mpsc::unbounded();
        let started = tokio::task::spawn_blocking({
            let sender = sender.clone();
            move || {
                // macOS reports events under the canonical path, which must match the root's.
                let root = fs::canonicalize(&handle.root).unwrap_or(handle.root);
                if let Ok(temps) = fs::read_dir(root.join(TMP)) {
                    for temp in temps.flatten() {
                        let _ = fs::remove_file(temp.path());
                    }
                }
                let (watcher, limited) = start(&root, sender);
                let scanned = scan(&root);
                (root, watcher, limited, scanned)
            }
        })
        .await;
        let Ok((root, mut watcher, mut limited, scanned)) = started else {
            return;
        };
        if limited {
            let _ = output.send(Watched::Limited).await;
        }
        let _ = output.send(Watched::Scan(scanned)).await;

        while let Some(result) = events.next().await {
            let batch = match result {
                Ok(batch) => batch,
                Err(errors) => {
                    for error in errors {
                        if matches!(error.kind, notify::ErrorKind::MaxFilesWatch) && !limited {
                            limited = true;
                            let _ = output.send(Watched::Limited).await;
                        } else {
                            tracing::debug!("watching {}: {error}", root.display());
                        }
                    }
                    continue;
                }
            };
            let rescan = batch.iter().any(|event| event.need_rescan());
            let paths = batch
                .into_iter()
                .flat_map(|event| event.event.paths)
                .collect::<BTreeSet<_>>();

            let reconciled = tokio::task::spawn_blocking({
                let root = root.clone();
                let sender = sender.clone();
                move || {
                    if !root.is_dir() {
                        return (watcher, false, Watched::Gone);
                    }
                    if rescan {
                        drop(watcher);
                        let (watcher, limited) = start(&root, sender);
                        let scan = scan(&root);
                        return (watcher, limited, Watched::Scan(scan));
                    }
                    let mut more = false;
                    let changes = reconcile(&root, &mut watcher, paths, &mut more);
                    (watcher, more, Watched::Changes(changes))
                }
            })
            .await;
            let Ok((next, more, watched)) = reconciled else {
                return;
            };
            watcher = next;
            if more && !limited {
                limited = true;
                let _ = output.send(Watched::Limited).await;
            }
            match watched {
                Watched::Gone => {
                    let _ = output.send(Watched::Gone).await;
                    std::future::pending::<()>().await;
                }
                Watched::Changes(changes) if changes.is_empty() => {}
                watched => {
                    let _ = output.send(watched).await;
                }
            }
        }
    })
}

/// A watcher of the folder at `root` sending to `sender`, with whether the system refused to
/// watch some of its directories.
fn start(
    root: &std::path::Path,
    sender: mpsc::UnboundedSender<DebounceEventResult>,
) -> (Option<Watcher>, bool) {
    let watcher = new_debouncer(DEBOUNCE, None, move |result| {
        let _ = sender.unbounded_send(result);
    });
    match watcher {
        Ok(mut watcher) => {
            let limited = watch_tree(&mut watcher, root, root);
            (Some(watcher), limited)
        }
        Err(error) => {
            tracing::warn!("cannot watch {}: {error}", root.display());
            (None, false)
        }
    }
}

/// Watches the directory `dir` of the folder at `root` with everything under it, returning whether
/// the system refused some directories. Each directory gets a watch of its own, which leaves the
/// skipped directories unwatched.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn watch_tree(watcher: &mut Watcher, _root: &std::path::Path, dir: &std::path::Path) -> bool {
    for dir in walk(dir).0 {
        if let Err(error) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
            if matches!(error.kind, notify::ErrorKind::MaxFilesWatch) {
                return true;
            }
            tracing::debug!("cannot watch {}: {error}", dir.display());
        }
    }
    false
}

/// Watches the folder at `root` with one watch on all of it, returning whether the system refused.
/// Skipped directories are watched too, and their events dropped. Every new watch restarts
/// FSEvents, so directories created later need none of their own.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn watch_tree(watcher: &mut Watcher, root: &std::path::Path, dir: &std::path::Path) -> bool {
    if dir != root {
        return false;
    }
    match watcher.watch(root, RecursiveMode::Recursive) {
        Ok(()) => false,
        Err(error) => {
            tracing::warn!("cannot watch {}: {error}", root.display());
            matches!(error.kind, notify::ErrorKind::MaxFilesWatch)
        }
    }
}

/// The changes behind events at `paths` in the folder at `root`, found by looking at each path
/// now: a file changed, a directory's files appeared, or the path is gone. New directories are
/// watched, setting `limited` when the system refuses. The packages bulin generates in the
/// unwatched `.bulin/` are reported removed when missing, so they can be written again.
fn reconcile(
    root: &std::path::Path,
    watcher: &mut Option<Watcher>,
    paths: BTreeSet<PathBuf>,
    limited: &mut bool,
) -> Vec<Change> {
    let mut changes = Vec::new();
    for file in paths {
        let Some(path) = relative(root, &file) else {
            continue;
        };
        match fs::symlink_metadata(&file) {
            Err(_) => changes.push(Change::Removed(path)),
            Ok(metadata) if metadata.is_dir() => {
                if let Some(watcher) = watcher {
                    *limited |= watch_tree(watcher, root, &file);
                }
                changes.extend(
                    walk(&file)
                        .1
                        .into_iter()
                        .filter_map(|file| Some(changed(relative(root, &file)?, &file))),
                );
            }
            Ok(_) => changes.push(changed(path, &file)),
        }
    }

    changes.extend(
        folder::generated("")
            .into_iter()
            .filter(|(path, _)| !root.join(path.as_str()).exists())
            .map(|(path, _)| Change::Removed(path)),
    );
    changes
}

/// Every file of the folder at `root` whose text storage reads, with its text.
fn scan(root: &std::path::Path) -> BTreeMap<Path, Option<String>> {
    walk(root)
        .1
        .into_iter()
        .filter_map(|file| {
            let path = relative(root, &file)?;
            reads_text(&path).then(|| (path.clone(), text(&path, &file)))
        })
        .collect()
}

/// The change of the file `path`, at `file` on disk.
fn changed(path: Path, file: &std::path::Path) -> Change {
    Change::Changed {
        text: text(&path, file),
        path,
    }
}

/// The text of the file `path`, at `file` on disk, when storage reads it and it is UTF-8 and at
/// most [`MAX_TEXT`] bytes.
fn text(path: &Path, file: &std::path::Path) -> Option<String> {
    if !reads_text(path) || fs::metadata(file).ok()?.len() > MAX_TEXT as u64 {
        return None;
    }
    String::from_utf8(fs::read(file).ok()?).ok()
}

/// The directories and files under `dir`, `dir` included, outside the skipped directories and
/// without following symlinks to directories.
fn walk(dir: &std::path::Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    let mut pending = vec![dir.to_owned()];

    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        dirs.push(dir);
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                let skipped = entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| SKIP.contains(&name));
                if !skipped {
                    pending.push(path);
                }
            } else if !kind.is_symlink() || fs::metadata(&path).is_ok_and(|target| target.is_file())
            {
                files.push(path);
            }
        }
    }
    (dirs, files)
}

/// `file` as a path of the folder at `root`, unless it is outside it, the root itself, skipped or
/// not a valid path.
fn relative(root: &std::path::Path, file: &std::path::Path) -> Option<Path> {
    let segments = file
        .strip_prefix(root)
        .ok()?
        .components()
        .map(|component| component.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()?;
    Path::new(&segments.join("/"))
        .ok()
        .filter(|path| !folder::is_skipped(path))
}

/// The storage error behind `error`.
fn error(error: io::Error) -> Error {
    match error.kind() {
        io::ErrorKind::NotFound => Error::Missing,
        io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => {
            Error::Denied(error.to_string())
        }
        io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => {
            Error::Quota(error.to_string())
        }
        _ => Error::Io(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::pin::Pin;
    use std::time::{Duration, Instant};

    use iced::futures::{Stream, StreamExt};
    use tempfile::TempDir;

    use super::*;

    type Files = BTreeMap<Path, Option<String>>;

    fn path(text: &str) -> Path {
        Path::new(text).unwrap()
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
    }

    fn folder(files: &[(&str, &str)]) -> (TempDir, Handle) {
        let dir = tempfile::tempdir().unwrap();
        for (file, text) in files {
            let file = dir.path().join(file);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        }
        let handle = Handle::new(dir.path().to_owned());
        (dir, handle)
    }

    /// What the watcher reported for each path, by applying every item to `files` until `done`
    /// holds or a generous deadline passes.
    fn follow(
        runtime: &tokio::runtime::Runtime,
        watched: &mut Pin<Box<impl Stream<Item = Watched>>>,
        files: &mut Files,
        done: impl Fn(&Files) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !done(files) {
            let left = deadline.saturating_duration_since(Instant::now());
            let item = runtime.block_on(tokio::time::timeout(left, watched.next()));
            match item {
                Ok(Some(Watched::Scan(scan))) => *files = scan,
                Ok(Some(Watched::Changes(changes))) => {
                    for change in changes {
                        match change {
                            Change::Changed { path, text } => {
                                files.insert(path, text);
                            }
                            Change::Removed(removed) => files.retain(|path, _| {
                                *path != removed && path.strip_prefix(&removed).is_none()
                            }),
                        }
                    }
                }
                Ok(Some(Watched::Gone | Watched::Limited)) | Ok(None) => {}
                Err(_) => panic!("the watcher didn't report it within 20 s: {files:?}"),
            }
        }
    }

    fn has(files: &Files, file: &str, text: Option<&str>) -> bool {
        files.get(&path(file)).map(Option::as_deref) == Some(text)
    }

    #[test]
    fn write_with_each_expect() {
        let runtime = runtime();
        let (dir, handle) = folder(&[("a.wesl", "a\r\n")]);
        let write = |file: &str, text: &str, expect: Expect| {
            runtime.block_on(super::write(
                handle.clone(),
                path(file),
                text.to_owned(),
                expect,
            ))
        };

        assert_eq!(
            write("new.wesl", "n", Expect::Missing),
            Ok(vec![Change::Changed {
                path: path("new.wesl"),
                text: Some("n".to_owned())
            }])
        );
        assert!(write("a.wesl", "b", Expect::Text("a\n".to_owned())).is_ok());
        assert_eq!(
            write("a.wesl", "c", Expect::Text("a".to_owned())),
            Err(Error::Conflict {
                disk: Some("b".to_owned())
            })
        );
        fs::remove_file(dir.path().join("a.wesl")).unwrap();
        assert_eq!(
            write("a.wesl", "c", Expect::Text("b".to_owned())),
            Err(Error::Conflict { disk: None })
        );
        assert_eq!(
            write("sub/notes.md", "md", Expect::Anything),
            Ok(vec![Change::Changed {
                path: path("sub/notes.md"),
                text: None
            }])
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("sub/notes.md")).unwrap(),
            "md"
        );
        assert_eq!(fs::read_to_string(dir.path().join("a.wesl")).ok(), None);
    }

    #[cfg(unix)]
    #[test]
    fn write_is_atomic_and_cleans_up() {
        use std::os::unix::fs::PermissionsExt;

        let runtime = runtime();
        let (dir, handle) = folder(&[("a.wesl", "a"), ("target.wesl", "t")]);
        let a = dir.path().join("a.wesl");
        fs::set_permissions(&a, fs::Permissions::from_mode(0o640)).unwrap();
        std::os::unix::fs::symlink(dir.path().join("target.wesl"), dir.path().join("link.wesl"))
            .unwrap();
        let write = |file: &str, text: &str| {
            runtime
                .block_on(super::write(
                    handle.clone(),
                    path(file),
                    text.to_owned(),
                    Expect::Anything,
                ))
                .unwrap()
        };

        write("a.wesl", "b");
        write("link.wesl", "through");

        assert_eq!(fs::read_to_string(&a).unwrap(), "b");
        assert_eq!(
            fs::metadata(&a).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert!(
            fs::symlink_metadata(dir.path().join("link.wesl"))
                .unwrap()
                .is_symlink()
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("target.wesl")).unwrap(),
            "through"
        );
        assert_eq!(fs::read_dir(dir.path().join(TMP)).unwrap().count(), 0);
    }

    #[test]
    fn read_rejects_large_and_binary_files() {
        let runtime = runtime();
        let (dir, handle) = folder(&[("a.wesl", "a")]);
        fs::write(dir.path().join("big.wesl"), vec![b'a'; MAX_TEXT + 1]).unwrap();
        fs::write(dir.path().join("binary.wesl"), [0xff, 0xfe]).unwrap();
        let read = |file: &str| runtime.block_on(super::read(handle.clone(), path(file)));

        assert_eq!(read("a.wesl"), Ok("a".to_owned()));
        assert_eq!(read("missing.wesl"), Err(Error::Missing));
        assert!(matches!(read("big.wesl"), Err(Error::Io(message)) if message.contains("1 MiB")));
        assert!(
            matches!(read("binary.wesl"), Err(Error::Io(message)) if message.contains("UTF-8"))
        );
    }

    #[test]
    fn scan_dir_lists_one_level_including_skip_list_dirs() {
        let runtime = runtime();
        let (_dir, handle) = folder(&[
            ("main.wesl", "main"),
            ("sub/lib.wesl", "lib"),
            (".git/HEAD", "ref"),
            ("target/y.wesl", "target"),
        ]);
        let entry = |kind, file| Entry {
            kind,
            path: path(file),
        };
        let scan = |dir: Option<&str>| {
            runtime
                .block_on(scan_dir(handle.clone(), dir.map(path)))
                .unwrap()
        };

        assert_eq!(
            scan(None),
            [
                entry(Kind::Dir, ".git"),
                entry(Kind::Dir, "sub"),
                entry(Kind::Dir, "target"),
                entry(Kind::File, "main.wesl"),
            ]
        );
        assert_eq!(scan(Some("sub")), [entry(Kind::File, "sub/lib.wesl")]);
        assert_eq!(
            runtime.block_on(scan_dir(handle.clone(), Some(path("missing")))),
            Err(Error::Missing)
        );
    }

    #[test]
    fn create_dir_then_scan_dir_lists_it() {
        let runtime = runtime();
        let (_dir, handle) = folder(&[("main.wesl", "main")]);

        assert_eq!(
            runtime.block_on(create_dir(handle.clone(), path("shaders"))),
            Ok(())
        );
        assert!(
            runtime
                .block_on(scan_dir(handle.clone(), None))
                .unwrap()
                .contains(&Entry {
                    kind: Kind::Dir,
                    path: path("shaders")
                })
        );
        assert!(matches!(
            runtime.block_on(create_dir(handle.clone(), path("shaders"))),
            Err(Error::Io(message)) if message.contains("already exists")
        ));
    }

    #[test]
    fn rename_a_file_and_a_directory() {
        let runtime = runtime();
        let (dir, handle) = folder(&[
            ("a.wesl", "a"),
            ("x.wesl", "x"),
            ("dir/b.wesl", "b"),
            ("dir/sub/c.md", "c"),
        ]);
        let rename = |from: &str, to: &str| {
            runtime.block_on(super::rename(handle.clone(), path(from), path(to)))
        };
        let changed = |file: &str, text: Option<&str>| Change::Changed {
            path: path(file),
            text: text.map(str::to_owned),
        };

        assert_eq!(
            rename("a.wesl", "z.wesl"),
            Ok(vec![
                Change::Removed(path("a.wesl")),
                changed("z.wesl", Some("a"))
            ])
        );

        let mut changes = rename("dir", "moved").unwrap();
        changes[1..].sort_by_key(|change| match change {
            Change::Changed { path, .. } | Change::Removed(path) => path.clone(),
        });
        assert_eq!(
            changes,
            [
                Change::Removed(path("dir")),
                changed("moved/b.wesl", Some("b")),
                changed("moved/sub/c.md", None),
            ]
        );
        assert!(!dir.path().join("dir").exists());

        assert!(matches!(
            rename("x.wesl", "z.wesl"),
            Err(Error::Io(message)) if message.contains("z.wesl already exists")
        ));
        assert_eq!(fs::read_to_string(dir.path().join("x.wesl")).unwrap(), "x");
        assert_eq!(fs::read_to_string(dir.path().join("z.wesl")).unwrap(), "a");
    }

    #[test]
    fn remove_a_file_and_a_directory() {
        let runtime = runtime();
        let (dir, handle) = folder(&[
            ("a.wesl", "a"),
            ("dir/b.wesl", "b"),
            ("dir/sub/c.wesl", "c"),
        ]);
        let remove = |file: &str| runtime.block_on(super::remove(handle.clone(), path(file)));

        assert_eq!(remove("a.wesl"), Ok(vec![Change::Removed(path("a.wesl"))]));
        assert_eq!(remove("dir"), Ok(vec![Change::Removed(path("dir"))]));
        assert!(!dir.path().join("a.wesl").exists());
        assert!(!dir.path().join("dir").exists());
        assert_eq!(remove("dir"), Err(Error::Missing));
    }

    #[test]
    fn the_watcher_reports_renames_as_content() {
        let runtime = runtime();
        let (_dir, handle) = folder(&[
            ("main.wesl", "main"),
            ("dir/a.wesl", "a"),
            ("dir/b.wesl", "b"),
        ]);
        let _guard = runtime.enter();
        let mut watched = Box::pin(stream(handle.clone()));
        let mut files = Files::new();
        follow(&runtime, &mut watched, &mut files, |files| files.len() == 3);

        let changes = runtime
            .block_on(super::rename(handle, path("dir"), path("moved")))
            .unwrap();
        let mut expected = files.clone();
        for change in changes {
            match change {
                Change::Changed { path, text } => {
                    expected.insert(path, text);
                }
                Change::Removed(removed) => expected
                    .retain(|path, _| *path != removed && path.strip_prefix(&removed).is_none()),
            }
        }
        assert!(has(&expected, "moved/a.wesl", Some("a")), "{expected:?}");

        follow(&runtime, &mut watched, &mut files, |files| {
            *files == expected
        });
    }

    #[test]
    fn watch_starts_with_a_scan() {
        let runtime = runtime();
        let (dir, handle) = folder(&[
            ("main.wesl", "main"),
            ("wesl.toml", "manifest"),
            ("sub/lib.wgsl", "lib"),
            ("notes.md", "notes"),
            (".git/x.wesl", "git"),
            (".bulin/uniforms/package.wesl", "uniforms"),
            ("target/y.wesl", "target"),
            (".bulin/tmp/0-0.tmp", "temp"),
        ]);
        let _guard = runtime.enter();
        let mut watched = Box::pin(stream(handle));

        let first = runtime.block_on(tokio::time::timeout(
            Duration::from_secs(20),
            watched.next(),
        ));
        let Ok(Some(Watched::Scan(scan))) = first else {
            panic!("the first item isn't a scan: {first:?}");
        };
        assert_eq!(
            scan,
            Files::from([
                (path("main.wesl"), Some("main".to_owned())),
                (path("sub/lib.wgsl"), Some("lib".to_owned())),
                (path("wesl.toml"), Some("manifest".to_owned())),
            ])
        );
        assert_eq!(fs::read_dir(dir.path().join(TMP)).unwrap().count(), 0);
    }

    #[test]
    fn watch_reports_writes_renames_and_directories() {
        let runtime = runtime();
        let (dir, handle) = folder(&[("main.wesl", "main")]);
        let root = dir.path();
        let _guard = runtime.enter();
        let mut watched = Box::pin(stream(handle));
        let mut files = Files::new();
        follow(&runtime, &mut watched, &mut files, |files| {
            has(files, "main.wesl", Some("main"))
        });

        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/x.wesl"), "git").unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("target/y.wesl"), "target").unwrap();
        fs::write(root.join("lib.wesl"), "lib").unwrap();
        follow(&runtime, &mut watched, &mut files, |files| {
            has(files, "lib.wesl", Some("lib"))
        });

        fs::write(root.join("main.wesl.new"), "renamed").unwrap();
        fs::rename(root.join("main.wesl.new"), root.join("main.wesl")).unwrap();
        follow(&runtime, &mut watched, &mut files, |files| {
            has(files, "main.wesl", Some("renamed"))
        });

        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub/a.wesl"), "a").unwrap();
        follow(&runtime, &mut watched, &mut files, |files| {
            has(files, "sub/a.wesl", Some("a"))
        });
        fs::write(root.join("sub/b.wesl"), "b").unwrap();
        follow(&runtime, &mut watched, &mut files, |files| {
            has(files, "sub/b.wesl", Some("b"))
        });

        fs::remove_dir_all(root.join("sub")).unwrap();
        follow(&runtime, &mut watched, &mut files, |files| {
            !files.contains_key(&path("sub/a.wesl")) && !files.contains_key(&path("sub/b.wesl"))
        });

        fs::write(root.join("notes.md"), "notes").unwrap();
        follow(&runtime, &mut watched, &mut files, |files| {
            has(files, "notes.md", None)
        });

        assert!(
            files.keys().all(|path| !folder::is_skipped(path)),
            "{files:?}"
        );
    }

    #[test]
    fn missing_generated_files_are_reported() {
        let runtime = runtime();
        let generated = folder::generated("").map(|(path, text)| (path.as_str().to_owned(), text));
        let files = generated
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect::<Vec<_>>();
        let (dir, handle) = folder(&files);
        let _guard = runtime.enter();
        let mut watched = Box::pin(stream(handle));
        let first = runtime.block_on(tokio::time::timeout(
            Duration::from_secs(20),
            watched.next(),
        ));
        assert!(matches!(first, Ok(Some(Watched::Scan(_)))), "{first:?}");

        fs::remove_dir_all(dir.path().join(".bulin")).unwrap();

        let deadline = Instant::now() + Duration::from_secs(20);
        let mut removed = BTreeSet::new();
        while removed.len() < generated.len() {
            let left = deadline.saturating_duration_since(Instant::now());
            match runtime.block_on(tokio::time::timeout(left, watched.next())) {
                Ok(Some(Watched::Changes(changes))) => {
                    removed.extend(changes.into_iter().filter_map(|change| match change {
                        Change::Removed(path) => Some(path.as_str().to_owned()),
                        Change::Changed { .. } => None,
                    }));
                }
                Ok(_) => {}
                Err(_) => panic!("only {removed:?} were reported removed within 20 s"),
            }
        }
        for (path, _) in &generated {
            assert!(removed.contains(path), "{path} isn't reported: {removed:?}");
        }
    }

    #[test]
    fn library_ids_are_none_natively() {
        let handle = Handle::new(PathBuf::from("/folders/sky"));
        let name = file::Name::from("/projects/sky.bulin");

        assert_eq!(handle.library_id(), None);
        assert_eq!(name.library_id(), None);
    }
}
