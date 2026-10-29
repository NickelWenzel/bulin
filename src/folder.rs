//! A folder the user opens: where its files are stored, the queue that writes them one at a time,
//! and the cell that keeps the document and the preview in step with them.
//!
//! Natively a folder is a directory; on the web it is an entry of the browser's library. Both
//! adapters offer the same functions and the same opener, so nothing outside this module knows
//! which one runs.

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use iced::futures::StreamExt;
use iced::{Subscription, Task};

#[cfg(not(target_arch = "wasm32"))]
use native::stream;
#[cfg(not(target_arch = "wasm32"))]
pub use native::{Handle, read, write};
#[cfg(target_arch = "wasm32")]
use web::stream;
#[cfg(target_arch = "wasm32")]
pub use web::{Handle, read, write};

use crate::action::Action;
use crate::analyzer::Analyzer;
use crate::core::folder::{
    Change, DEPENDENCY_LINES, Expect, MANIFEST, Mirror, Path, SETTINGS, Settings,
    declares_dependencies, generated, is_shader, package_root, root_manifest, same_text,
};
use crate::core::program::{self, Module, Program};
use crate::core::{self, Clock, Uniforms};
use crate::document;

/// Why storage couldn't read or write a file. It holds only text, so the messages carrying it can
/// cross threads.
#[derive(thiserror::Error, Debug, Clone, PartialEq)]
pub enum Error {
    #[error("the file doesn't exist")]
    Missing,
    /// The file isn't what the write expected: it holds `disk`, or nothing when `None`.
    #[error("the file changed since bulin last read it")]
    Conflict { disk: Option<String> },
    #[error("bulin may not write there ({0})")]
    Denied(String),
    #[error("there is no space left ({0}); free some and save again")]
    Quota(String),
    #[error("{0}")]
    Io(String),
}

/// What watching a folder reports.
#[derive(Debug, Clone)]
pub enum Watched {
    /// Every file whose text storage reads, read afresh: shader files and `wesl.toml` files
    /// outside the skipped directories, with `None` for text that is too large or not UTF-8.
    Scan(BTreeMap<Path, Option<String>>),
    /// Files changed by someone, bulin included.
    #[cfg(not(target_arch = "wasm32"))]
    Changes(Vec<Change>),
    /// The folder no longer exists.
    Gone,
    /// The system watches no more directories, so changes in some aren't seen.
    #[cfg(not(target_arch = "wasm32"))]
    Limited,
}

/// One file to write, carrying its folder so it can never land in another.
#[derive(Debug, Clone)]
pub struct Write {
    pub handle: Handle,
    pub path: Path,
    pub text: String,
    pub expect: Expect,
}

/// The folder's writes, in order, one at a time: the head is in flight until its completion
/// arrives, so later writes can expect what it leaves on disk.
#[derive(Debug, Default)]
pub struct Queue {
    writes: VecDeque<Write>,
}

/// An open folder: its storage, the texts the preview compiles, its settings and its writes.
pub struct State {
    handle: Handle,
    /// Numbers this opening of the folder, so results meant for an earlier one are dropped.
    session: u64,
    mirror: Mirror,
    /// Whether the first scan arrived. Changes before it are dropped, since the scan replaces
    /// the mirror anyway.
    scanned: bool,
    queue: Queue,
    settings: Settings,
    /// Whether `settings.json` couldn't be read, so writing it would lose what it holds.
    settings_locked: bool,
    /// Whether storage refused a write, after which bulin writes only what the user saves: one
    /// read-only file mustn't keep the others from being saved.
    read_only: bool,
    /// Whether the folder no longer exists, after which bulin writes nothing.
    gone: bool,
    /// The text of the root `wesl.toml`.
    manifest: Option<String>,
    /// The package root `manifest` declares, `None` for the folder itself.
    root: Option<Path>,
    /// The modules the last compile read, `None` when it failed before telling.
    read: Option<BTreeSet<Module>>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Watched(u64, Watched),
    Wrote {
        session: u64,
        path: Path,
        text: String,
        result: Result<Vec<Change>, Error>,
    },
    Read {
        session: u64,
        path: Path,
        result: Result<String, Error>,
    },
}

pub enum Instruction {
    /// Files the program may read changed: build and show it again.
    ProgramChanged,
    /// Every queued write finished.
    Drained,
}

/// Picking a folder or a project to open: the system's dialogs natively, the library on the web.
pub mod opener {
    use std::sync::Arc;

    #[cfg(not(target_arch = "wasm32"))]
    pub use super::native::{Message, Opener};
    #[cfg(target_arch = "wasm32")]
    pub use super::web::{Message, Opener};

    use super::Handle;
    use crate::file;

    /// What the user picked.
    pub enum Instruction {
        OpenFolder(Handle),
        /// A project file and its text, not yet parsed.
        OpenProject(file::Name, Arc<String>),
        /// Why opening failed, for the user.
        Failed(String),
    }
}

impl Queue {
    /// Queues `write`, returning the write to start now, if any. A queued write to the same path
    /// that hasn't started takes `write`'s text instead, and keeps its expectation, which is what
    /// the writes before it leave on disk.
    pub fn push(&mut self, write: Write) -> Option<Write> {
        if let Some(queued) = self
            .writes
            .iter_mut()
            .skip(1)
            .rev()
            .find(|queued| queued.path == write.path)
        {
            queued.text = write.text;
            return None;
        }
        self.writes.push_back(write);
        (self.writes.len() == 1).then(|| self.writes[0].clone())
    }

    /// Queues the user's save of `path`, returning the write to start now, if any. It expects
    /// the newest queued text for the path, or else `base`, the text last loaded or saved, with
    /// `None` meaning no file.
    pub fn save(
        &mut self,
        handle: Handle,
        path: Path,
        text: String,
        base: Option<String>,
    ) -> Option<Write> {
        let expect = match self.newest(&path) {
            Some(newest) => Expect::Text(newest.to_owned()),
            None => base.map_or(Expect::Missing, Expect::Text),
        };
        self.push(Write {
            handle,
            path,
            text,
            expect,
        })
    }

    /// Finishes the write in flight, returning the next one to start. A failed write takes the
    /// queued writes to its path with it, since they expect what it would have written.
    pub fn complete(&mut self, ok: bool) -> Option<Write> {
        if let Some(done) = self.writes.pop_front()
            && !ok
        {
            self.writes.retain(|write| write.path != done.path);
        }
        self.writes.front().cloned()
    }

    /// The newest text queued or in flight for `path`.
    pub fn newest(&self, path: &Path) -> Option<&str> {
        self.writes
            .iter()
            .rev()
            .find(|write| write.path == *path)
            .map(|write| write.text.as_str())
    }

    /// Whether a change of `path` to `text` is one of the writes queued or in flight.
    pub fn is_echo(&self, path: &Path, text: &str) -> bool {
        self.writes
            .iter()
            .any(|write| write.path == *path && same_text(&write.text, text))
    }

    pub fn is_empty(&self) -> bool {
        self.writes.is_empty()
    }
}

impl State {
    /// Opens the folder at `handle` as the `session`th opening, with the `settings` read from
    /// it. `settings_locked` says they couldn't be read, so they are never written. Writes the
    /// files bulin generates and a root `wesl.toml` when there is none.
    pub fn new(
        handle: Handle,
        session: u64,
        settings: Settings,
        settings_locked: bool,
    ) -> (Self, Task<Message>) {
        let mut state = Self {
            handle,
            session,
            mirror: Mirror::default(),
            scanned: false,
            queue: Queue::default(),
            settings,
            settings_locked,
            read_only: false,
            gone: false,
            manifest: None,
            root: None,
            read: None,
        };
        let generate = state.generate();
        let settings = state.persist();
        // A conflict means the folder has its own manifest, which stays as it is.
        let manifest = state.enqueue(manifest(), root_manifest(), Expect::Missing);

        (state, Task::batch([generate, settings, manifest]))
    }

    /// Applies `message`. Opening the main file replaces `document`, and changes on disk reach it
    /// by the rules for unsaved changes. `notice` receives what the user should know.
    pub fn update(
        &mut self,
        message: Message,
        document: &mut document::State,
        analyzer: &mut Analyzer,
        settings: &core::Settings,
        notice: &mut Option<String>,
    ) -> Action<Instruction, Message> {
        match message {
            Message::Watched(session, _)
            | Message::Wrote { session, .. }
            | Message::Read { session, .. }
                if session != self.session =>
            {
                Action::none()
            }
            Message::Watched(_, Watched::Scan(scan)) => {
                let changes = self.mirror.replace(&scan);
                let (relevant, task) = self.apply(&changes, document, analyzer, notice);
                let first = !self.scanned;
                self.scanned = true;

                if first
                    && let Some(main) = &self.settings.main
                    && let Some(text) = self.mirror.files().get(main)
                {
                    document.detach(analyzer);
                    *document = document::State::folder(main.clone(), text, settings);
                    document.attach(analyzer);
                }

                Action {
                    instruction: (first || relevant).then_some(Instruction::ProgramChanged),
                    task,
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            Message::Watched(_, Watched::Changes(changes)) => {
                if !self.scanned {
                    return Action::none();
                }
                let (relevant, task) = self.apply(&changes, document, analyzer, notice);
                Action {
                    instruction: relevant.then_some(Instruction::ProgramChanged),
                    task,
                }
            }
            Message::Watched(_, Watched::Gone) => {
                self.gone = true;
                Action::none()
            }
            #[cfg(not(target_arch = "wasm32"))]
            Message::Watched(_, Watched::Limited) => {
                *notice = Some(
                    "bulin can't watch every directory of this folder, so it misses some changes. \
                     Raising fs.inotify.max_user_watches lets it watch more."
                        .to_owned(),
                );
                Action::none()
            }
            Message::Wrote {
                path, text, result, ..
            } => {
                let next = self.queue.complete(result.is_ok());
                let mut task = next.map_or_else(Task::none, |next| self.start(next));
                let mut relevant = false;

                match result {
                    Ok(changes) => {
                        document.saved(&path, &Ok(text));
                        if self.scanned {
                            let applied = self.apply(&changes, document, analyzer, notice);
                            relevant = applied.0;
                            task = Task::batch([task, applied.1]);
                        }
                    }
                    // The folder has a manifest of its own.
                    Err(Error::Conflict { .. }) if path == manifest() => {}
                    Err(Error::Conflict { disk }) => {
                        let deleted = disk.is_none();
                        document.saved(&path, &Err(Error::Conflict { disk }));
                        let (what, again) = if deleted {
                            ("was deleted", "recreates")
                        } else {
                            ("changed", "overwrites")
                        };
                        *notice = Some(format!(
                            "{path} {what} on disk, so it wasn't saved. Saving again {again} it."
                        ));
                    }
                    Err(Error::Denied(reason)) => {
                        if !self.read_only {
                            self.read_only = true;
                            *notice = Some(format!(
                                "bulin may not write to this folder ({reason}), so it stops \
                                 writing its own files there."
                            ));
                        }
                    }
                    Err(error) => *notice = Some(format!("Could not save {path}: {error}")),
                }

                let instruction = if relevant {
                    Some(Instruction::ProgramChanged)
                } else {
                    self.queue.is_empty().then_some(Instruction::Drained)
                };
                Action { instruction, task }
            }
            Message::Read { path, result, .. } => {
                if !matches!(document.location(), document::Location::Folder(open) if *open == path)
                {
                    return Action::none();
                }
                match result {
                    Ok(text) => self.follow(&path, Some(&text), document, analyzer, notice),
                    Err(Error::Missing) => self.follow(&path, None, document, analyzer, notice),
                    Err(error) => *notice = Some(format!("Could not read {path}: {error}")),
                }
                Action::none()
            }
        }
    }

    /// Watches the folder for changes, starting with a scan of its files.
    pub fn subscription(&self) -> Subscription<Message> {
        watch(self.session, &self.handle)
            .map(|(session, watched)| Message::Watched(session, watched))
    }

    /// Queues the user's save of `path` with `text`, which was `base` when last loaded or saved.
    /// Refused with a `notice` once the folder is gone.
    pub fn save(
        &mut self,
        path: Path,
        text: String,
        base: Option<String>,
        notice: &mut Option<String>,
    ) -> Task<Message> {
        if self.gone {
            *notice = Some(format!("{path} can't be saved: the folder is gone."));
            return Task::none();
        }
        self.queue
            .save(self.handle.clone(), path, text, base)
            .map_or_else(Task::none, |next| self.start(next))
    }

    /// The program of the main file with `uniforms` and `time`, or `None` while there is no main
    /// file.
    pub fn program(
        &self,
        uniforms: Uniforms,
        time: Option<f32>,
    ) -> Option<Result<Program, program::Error>> {
        let main = self.settings.main.as_ref()?;
        Some(Program::folder(
            Arc::clone(self.mirror.files()),
            self.root.clone(),
            main,
            uniforms,
            time,
        ))
    }

    /// Takes in the modules the last compile read, `None` when it failed before telling.
    pub fn compiled(&mut self, read: Option<BTreeSet<Module>>) {
        self.read = read;
    }

    /// Keeps the settings' uniforms and clock and writes the uniforms package for their layout.
    pub fn layout_changed(&mut self, uniforms: &Uniforms, clock: Option<Clock>) -> Task<Message> {
        self.settings.uniforms = uniforms.clone();
        self.settings.clock = clock;
        self.generate()
    }

    /// Whether the first scan arrived, so the folder's files are known.
    pub fn is_scanned(&self) -> bool {
        self.scanned
    }

    /// Whether no write is queued or in flight.
    pub fn is_idle(&self) -> bool {
        self.queue.is_empty()
    }

    /// The folder's name, for a title.
    pub fn name(&self) -> &str {
        self.handle.name()
    }

    /// What stays wrong with the folder, for as long as it does.
    pub fn status(&self) -> Option<&str> {
        self.gone
            .then_some("The folder is gone, so bulin saves nothing. Close it to go on.")
    }

    /// The write in flight.
    #[cfg(test)]
    pub fn in_flight(&self) -> Option<&Write> {
        self.queue.writes.front()
    }

    /// Applies `changes` to the mirror, the root manifest and the document, returning whether the
    /// program may read a changed file and the reads and writes they call for.
    fn apply(
        &mut self,
        changes: &[Change],
        document: &mut document::State,
        analyzer: &mut Analyzer,
        notice: &mut Option<String>,
    ) -> (bool, Task<Message>) {
        let generated = generated("").map(|(path, _)| path);
        let mut relevant = false;
        let mut regenerate = false;
        let mut tasks = Vec::new();

        for change in changes {
            for (path, _) in self.mirror.apply(change) {
                relevant |= self.is_relevant(&path);
            }

            let document::Location::Folder(open) = document.location().clone() else {
                continue;
            };
            match change {
                Change::Changed { path, text } if *path == open => match text {
                    Some(text) => {
                        self.follow(&open, Some(text.as_str()), document, analyzer, notice)
                    }
                    // A queued write's text would be bulin's own, an echo.
                    None if self.queue.newest(path).is_some() => {}
                    None => {
                        let session = self.session;
                        let path = path.clone();
                        tasks.push(Task::perform(
                            read(self.handle.clone(), path.clone()),
                            move |result| Message::Read {
                                session,
                                path,
                                result,
                            },
                        ));
                    }
                },
                Change::Removed(path) if *path == open || open.strip_prefix(path).is_some() => {
                    self.follow(&open, None, document, analyzer, notice);
                }
                Change::Changed { .. } | Change::Removed(_) => {}
            }

            regenerate |= matches!(change, Change::Removed(path) if generated.contains(path));
        }

        let manifest = self.mirror.files().get(&manifest()).cloned();
        if manifest != self.manifest {
            self.root = match manifest.as_deref().map(|text| (text, package_root(text))) {
                None => None,
                Some((text, Ok(root))) => {
                    if !declares_dependencies(text) {
                        let lines = DEPENDENCY_LINES.trim_end().replace('\n', ", ");
                        *notice = Some(format!(
                            "wesl.toml lacks bulin's packages. Add to its [dependencies]: {lines}"
                        ));
                    }
                    root
                }
                Some((_, Err(error))) => {
                    *notice = Some(format!("{error}. The folder is the package root."));
                    None
                }
            };
            self.manifest = manifest;
        }

        if regenerate {
            tasks.push(self.generate());
            tasks.push(self.persist());
        }
        (relevant, Task::batch(tasks))
    }

    /// Brings the open folder file `path` in line with storage now holding `text`, `None` when
    /// the file is gone. Unsaved changes are kept, and the user is told the file changed.
    fn follow(
        &self,
        path: &Path,
        text: Option<&str>,
        document: &mut document::State,
        analyzer: &mut Analyzer,
        notice: &mut Option<String>,
    ) {
        let Some(text) = text else {
            if document.base().is_some() {
                document.disk_changed(path, None);
                *notice = Some(format!(
                    "{path} was deleted on disk. Saving it again recreates it."
                ));
            }
            return;
        };

        // An echo's completion updates the document, and a base equal to `text` is up to date.
        if self.queue.is_echo(path, text)
            || document.base().is_some_and(|base| same_text(base, text))
        {
            return;
        }
        if same_text(text, &document.content()) {
            document.disk_changed(path, Some(text));
        } else if !document.is_dirty() {
            document.reloaded(path, text, analyzer);
        } else {
            document.disk_changed(path, Some(text));
            *notice = Some(format!(
                "{path} changed on disk. Saving it again overwrites that."
            ));
        }
    }

    /// Whether the program may read the file `path`, which changed.
    fn is_relevant(&self, path: &Path) -> bool {
        *path == manifest()
            || self.settings.main.as_ref() == Some(path)
            || match &self.read {
                Some(read) => {
                    Module::of(self.root.as_ref(), path).is_ok_and(|module| read.contains(&module))
                }
                None => is_shader(path),
            }
    }

    /// Writes the packages bulin generates, unless bulin doesn't write to the folder.
    fn generate(&mut self) -> Task<Message> {
        if self.read_only || self.gone {
            return Task::none();
        }
        let package = program::uniforms_package(
            self.settings.clock.map(|clock| clock.seconds()),
            &self.settings.uniforms,
        );
        let tasks = generated(&package)
            .into_iter()
            .map(|(path, text)| self.enqueue(path, text, Expect::Anything))
            .collect::<Vec<_>>();
        Task::batch(tasks)
    }

    /// Writes `settings.json`, unless it couldn't be read or bulin doesn't write to the folder.
    fn persist(&mut self) -> Task<Message> {
        if self.settings_locked || self.read_only || self.gone {
            return Task::none();
        }
        let settings = Path::new(SETTINGS).expect("invariant: the settings path is valid");
        self.enqueue(settings, self.settings.to_json(), Expect::Anything)
    }

    /// Queues a write of `text` to `path`, starting it when nothing else is in flight.
    fn enqueue(&mut self, path: Path, text: String, expect: Expect) -> Task<Message> {
        let write = Write {
            handle: self.handle.clone(),
            path,
            text,
            expect,
        };
        self.queue
            .push(write)
            .map_or_else(Task::none, |next| self.start(next))
    }

    /// Runs `next`, the write the queue just put in flight.
    fn start(&self, next: Write) -> Task<Message> {
        let session = self.session;
        let Write {
            handle,
            path,
            text,
            expect,
        } = next;

        Task::perform(
            write(handle, path.clone(), text.clone(), expect),
            move |result| Message::Wrote {
                session,
                path,
                text,
                result,
            },
        )
    }
}

/// Watches the folder at `handle` for the `session`th opening: first a scan of its files, then
/// what changes, each tagged with `session`.
pub fn watch(session: u64, handle: &Handle) -> Subscription<(u64, Watched)> {
    Subscription::run_with((session, handle.clone()), |(session, handle)| {
        let session = *session;
        stream(handle.clone()).map(move |watched| (session, watched))
    })
}

/// The root `wesl.toml`.
fn manifest() -> Path {
    Path::new(MANIFEST).expect("invariant: the manifest path is valid")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use iced::time::Instant;
    use matcha_iced::lsp::client::Events;

    use super::*;

    fn path(text: &str) -> Path {
        Path::new(text).unwrap()
    }

    fn handle() -> Handle {
        Handle::new(PathBuf::from("/folder"))
    }

    fn write(file: &str, text: &str) -> Write {
        Write {
            handle: handle(),
            path: path(file),
            text: text.to_owned(),
            expect: Expect::Anything,
        }
    }

    fn changed(file: &str, text: Option<&str>) -> Change {
        Change::Changed {
            path: path(file),
            text: text.map(str::to_owned),
        }
    }

    /// An open folder with its main file in the document, the initial writes done.
    struct Open {
        state: State,
        document: document::State,
        analyzer: Analyzer,
        _events: Events,
        notice: Option<String>,
    }

    impl Open {
        fn new(files: &[(&str, &str)]) -> Self {
            let settings = Settings::new(Some(path("main.wesl")), Uniforms::default(), None);
            let (mut state, _) = State::new(handle(), 1, settings, false);
            while state.queue.complete(true).is_some() {}
            let (analyzer, events) = Analyzer::start();
            let mut open = Self {
                state,
                document: document::State::new("", &core::Settings::default()),
                analyzer,
                _events: events,
                notice: None,
            };

            let scan = files
                .iter()
                .map(|(file, text)| (path(file), Some((*text).to_owned())))
                .collect();
            let instruction = open.update(Message::Watched(1, Watched::Scan(scan)));
            assert!(matches!(instruction, Some(Instruction::ProgramChanged)));
            open
        }

        fn update(&mut self, message: Message) -> Option<Instruction> {
            self.state
                .update(
                    message,
                    &mut self.document,
                    &mut self.analyzer,
                    &core::Settings::default(),
                    &mut self.notice,
                )
                .instruction
        }

        fn changes(&mut self, changes: Vec<Change>) -> Option<Instruction> {
            self.update(Message::Watched(1, Watched::Changes(changes)))
        }

        /// Types `text` at the caret, which starts at the start of the document.
        fn type_text(&mut self, text: &str) {
            for character in text.chars() {
                let _ = self.document.update(
                    document::Message::Editor(matcha_iced::Event::Editor(
                        matcha_iced::Action::Type(character),
                    )),
                    Instant::now(),
                    &mut self.analyzer,
                    &core::Settings::default(),
                );
            }
        }

        /// Saves the document, returning the text the write carries.
        fn save(&mut self) -> String {
            let action = self.document.update(
                document::Message::SaveFile,
                Instant::now(),
                &mut self.analyzer,
                &core::Settings::default(),
            );
            let Some(document::Instruction::Save { path, text, base }) = action.instruction else {
                panic!("a folder file saves through the folder");
            };
            let _ = self.state.save(path, text.clone(), base, &mut self.notice);
            text
        }

        fn wrote(&mut self, file: &str, text: &str, result: Result<Vec<Change>, Error>) {
            let _ = self.update(Message::Wrote {
                session: 1,
                path: path(file),
                text: text.to_owned(),
                result,
            });
        }
    }

    fn main_module() -> Module {
        Module::of(None, &path("main.wesl")).unwrap()
    }

    #[test]
    fn two_quick_saves_dont_conflict() {
        let mut queue = Queue::default();
        let a = queue
            .save(handle(), path("main.wesl"), "a".into(), Some("base".into()))
            .unwrap();
        assert_eq!(a.expect, Expect::Text("base".into()));

        assert!(
            queue
                .save(handle(), path("main.wesl"), "b".into(), Some("base".into()))
                .is_none()
        );
        let b = queue.complete(true).unwrap();
        assert_eq!(b.text, "b");
        assert_eq!(b.expect, Expect::Text("a".into()));
    }

    #[test]
    fn a_coalesced_save_keeps_the_replaced_expectation() {
        let mut queue = Queue::default();
        let _ = queue.save(handle(), path("main.wesl"), "a".into(), None);
        let _ = queue.save(handle(), path("main.wesl"), "b".into(), None);
        let _ = queue.save(handle(), path("main.wesl"), "c".into(), None);

        let next = queue.complete(true).unwrap();
        assert_eq!(next.text, "c");
        assert_eq!(next.expect, Expect::Text("a".into()));
        assert!(queue.complete(true).is_none());
    }

    #[test]
    fn an_echo_after_more_typing_is_recognized() {
        let mut queue = Queue::default();
        let _ = queue.save(handle(), path("main.wesl"), "a\nb".into(), None);

        assert!(queue.is_echo(&path("main.wesl"), "a\r\nb"));
        assert!(!queue.is_echo(&path("main.wesl"), "a\nbc"));
        assert!(!queue.is_echo(&path("other.wesl"), "a\nb"));
    }

    #[test]
    fn a_failed_write_drops_later_writes_to_the_same_path() {
        let mut queue = Queue::default();
        let _ = queue.push(write("main.wesl", "a"));
        let _ = queue.push(write("other.wesl", "x"));
        let _ = queue.push(write("main.wesl", "b"));

        let next = queue.complete(false).unwrap();
        assert_eq!(next.path, path("other.wesl"));
        assert_eq!(queue.newest(&path("main.wesl")), None);
        assert!(queue.complete(true).is_none());
        assert!(queue.is_empty());
    }

    #[test]
    fn writes_start_in_order() {
        let mut queue = Queue::default();
        let first = queue.push(write("b.wesl", "1")).unwrap();
        assert_eq!(first.path, path("b.wesl"));
        assert!(queue.push(write("a.wesl", "2")).is_none());
        assert!(queue.push(write("c.wesl", "3")).is_none());

        let order = std::iter::from_fn(|| queue.complete(true))
            .map(|write| write.path.as_str().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(order, ["a.wesl", "c.wesl"]);
    }

    #[test]
    fn a_stale_completion_still_advances_the_queue() {
        let mut open = Open::new(&[("main.wesl", "a")]);
        open.type_text("x");
        let text = open.save();
        open.document = document::State::new("elsewhere", &core::Settings::default());

        let changes = vec![changed("main.wesl", Some(&text))];
        let _ = open.update(Message::Wrote {
            session: 1,
            path: path("main.wesl"),
            text,
            result: Ok(changes),
        });

        assert!(open.state.is_idle());
        assert_eq!(open.document.content(), "elsewhere");
    }

    #[test]
    fn an_echo_changes_nothing() {
        let mut open = Open::new(&[("main.wesl", "a")]);
        open.type_text("x");
        let text = open.save();
        assert!(open.document.is_dirty());

        let changes = vec![changed("main.wesl", Some(&text))];
        open.wrote("main.wesl", &text, Ok(changes.clone()));
        assert!(!open.document.is_dirty());

        assert!(open.changes(changes).is_none());
        assert!(!open.document.is_dirty());
        assert_eq!(open.notice, None);
    }

    #[test]
    fn an_unrelated_change_doesnt_recompile() {
        let mut open = Open::new(&[("main.wesl", "a"), ("other.wesl", "o")]);
        open.state.compiled(Some(BTreeSet::from([main_module()])));

        assert!(
            open.changes(vec![changed("other.wesl", Some("o2"))])
                .is_none()
        );
        assert!(open.changes(vec![changed("new.wesl", Some("n"))]).is_none());
    }

    #[test]
    fn a_relevant_change_recompiles() {
        let mut open = Open::new(&[("main.wesl", "a"), ("lib.wesl", "l")]);
        let lib = Module::of(None, &path("lib.wesl")).unwrap();
        open.state
            .compiled(Some(BTreeSet::from([main_module(), lib])));

        assert!(matches!(
            open.changes(vec![changed("lib.wesl", Some("l2"))]),
            Some(Instruction::ProgramChanged)
        ));
        assert!(matches!(
            open.changes(vec![Change::Removed(path("lib.wesl"))]),
            Some(Instruction::ProgramChanged)
        ));
        assert!(matches!(
            open.changes(vec![changed("wesl.toml", Some(&root_manifest()))]),
            Some(Instruction::ProgramChanged)
        ));
    }

    #[test]
    fn a_failed_compile_makes_every_shader_change_relevant() {
        let mut open = Open::new(&[("main.wesl", "a"), ("other.wesl", "o")]);
        open.state.compiled(None);

        assert!(matches!(
            open.changes(vec![changed("other.wesl", Some("o2"))]),
            Some(Instruction::ProgramChanged)
        ));
        assert!(open.changes(vec![changed("notes.md", None)]).is_none());
    }

    #[test]
    fn a_clean_document_reloads_silently() {
        let mut open = Open::new(&[("main.wesl", "a")]);

        let _ = open.changes(vec![changed("main.wesl", Some("b"))]);

        assert_eq!(open.document.content(), "b");
        assert_eq!(open.document.base(), Some("b"));
        assert!(!open.document.is_dirty());
        assert_eq!(open.notice, None);
    }

    #[test]
    fn a_dirty_document_gets_a_notice_and_keeps_its_text() {
        let mut open = Open::new(&[("main.wesl", "a")]);
        open.type_text("x");

        let _ = open.changes(vec![changed("main.wesl", Some("b"))]);

        assert_eq!(open.document.content(), "xa");
        assert_eq!(open.document.base(), Some("b"));
        assert!(open.document.is_dirty());
        assert!(open.notice.as_deref().unwrap().contains("changed on disk"));

        open.notice = None;
        let _ = open.changes(vec![Change::Removed(path("main.wesl"))]);
        assert_eq!(open.document.base(), None);
        assert!(open.notice.as_deref().unwrap().contains("deleted"));
    }

    #[test]
    fn a_conflict_on_save_sets_base_to_disk() {
        let mut open = Open::new(&[("main.wesl", "a")]);
        open.type_text("x");
        let text = open.save();

        let disk = Some("disk".to_owned());
        open.wrote("main.wesl", &text, Err(Error::Conflict { disk }));

        assert_eq!(open.document.base(), Some("disk"));
        assert!(open.document.is_dirty());
        assert!(open.notice.is_some());
        assert!(open.state.is_idle());
    }

    #[test]
    fn a_refused_write_stops_generated_writes_but_not_saves() {
        let mut open = Open::new(&[("main.wesl", "a")]);
        let _ = open.state.layout_changed(&Uniforms::default(), None);
        let refused = open.state.in_flight().unwrap().clone();
        open.wrote(
            refused.path.as_str(),
            &refused.text,
            Err(Error::Denied("read-only".to_owned())),
        );
        assert!(open.notice.take().is_some());
        while open.state.queue.complete(true).is_some() {}

        let _ = open.state.layout_changed(&Uniforms::default(), None);
        assert!(
            open.state.is_idle(),
            "generated files aren't written any more"
        );

        open.type_text("x");
        let text = open.save();
        assert_eq!(
            open.state.queue.newest(&path("main.wesl")),
            Some(text.as_str())
        );
        assert_eq!(open.notice, None);
    }

    #[test]
    fn changes_before_the_first_scan_are_discarded() {
        let settings = Settings::new(Some(path("main.wesl")), Uniforms::default(), None);
        let (mut state, _) = State::new(handle(), 1, settings, false);
        let (mut analyzer, _events) = Analyzer::start();
        let mut document = document::State::new("", &core::Settings::default());
        let mut notice = None;
        let mut update = |state: &mut State, message| {
            state
                .update(
                    message,
                    &mut document,
                    &mut analyzer,
                    &core::Settings::default(),
                    &mut notice,
                )
                .instruction
        };

        let changes = vec![changed("main.wesl", Some("early"))];
        assert!(update(&mut state, Message::Watched(1, Watched::Changes(changes))).is_none());
        assert!(state.mirror.files().is_empty());

        let scan = BTreeMap::from([(path("main.wesl"), Some("scanned".to_owned()))]);
        assert!(update(&mut state, Message::Watched(1, Watched::Scan(scan))).is_some());
        assert_eq!(document.content(), "scanned");
    }

    #[test]
    fn a_stale_session_is_dropped() {
        let mut open = Open::new(&[("main.wesl", "a")]);
        let in_flight = open.state.queue.writes.len();
        open.type_text("x");
        let _ = open.save();

        let stale = vec![changed("main.wesl", Some("stale"))];
        assert!(
            open.update(Message::Watched(0, Watched::Changes(stale.clone())))
                .is_none()
        );
        assert!(
            open.update(Message::Wrote {
                session: 0,
                path: path("main.wesl"),
                text: "stale".to_owned(),
                result: Ok(stale),
            })
            .is_none()
        );

        assert_eq!(open.state.queue.writes.len(), in_flight + 1);
        assert_eq!(open.state.mirror.files()[&path("main.wesl")], "a");
        assert_eq!(open.document.content(), "xa");
    }

    #[test]
    fn a_crlf_echo_is_still_an_echo() {
        let mut open = Open::new(&[("main.wesl", "a\nb")]);
        open.type_text("x\n");
        let text = open.save();
        open.type_text("y");

        let _ = open.changes(vec![changed(
            "main.wesl",
            Some(&text.replace('\n', "\r\n")),
        )]);

        assert_eq!(open.notice, None);
        assert_eq!(open.document.content(), "x\nya\nb");
        assert_eq!(open.document.base(), Some("a\nb"));
    }
}
