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
use std::time::Duration;

use iced::futures::StreamExt;
use iced::time::{self, Instant};
use iced::{Element, Subscription, Task};

#[cfg(not(target_arch = "wasm32"))]
use native::stream;
#[cfg(not(target_arch = "wasm32"))]
pub use native::{Handle, read, scan_dir, write};
#[cfg(target_arch = "wasm32")]
use web::stream;
#[cfg(target_arch = "wasm32")]
pub use web::{Handle, read, scan, scan_dir, write};

use crate::action::Action;
use crate::analyzer::Analyzer;
use crate::core::folder::{
    Change, DEPENDENCY_LINES, Effect, Entry, Expect, MANIFEST, Mirror, Path, SETTINGS, Settings,
    declares_dependencies, generated, is_shader, is_skipped, package_root, root_manifest,
    same_text,
};
use crate::core::program::{self, Module, Program};
use crate::core::{self, Clock, Uniforms};
use crate::prompt::Prompt;
use crate::{Pending, document, explorer};

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
    /// When the user last edited a value the settings hold, until the settings are written.
    settings_dirty_since: Option<Instant>,
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
    explorer: explorer::State,
    /// The file being read to become the document, until a later open supersedes it, and whether
    /// the document's unsaved changes were given up for it.
    opening: Option<(Path, bool)>,
    /// Whether the user is answering a question, during which changes on disk leave the document
    /// alone until [`State::release`] checks it again.
    held: bool,
    /// What storage last reported for a file since it became the document, `None` text meaning
    /// the file is gone. Without a report, storage holds the document's `base`.
    disk: Option<(Path, Option<String>)>,
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
    Explorer(explorer::Message),
    /// The listing of the directory `dir`, `None` being the folder itself.
    Listed {
        session: u64,
        dir: Option<Path>,
        result: Result<Vec<Entry>, Error>,
    },
    /// The text of the file `path`, read to open it as the document.
    Opened {
        session: u64,
        path: Path,
        result: Result<String, Error>,
    },
    /// Time to check whether the settings were last edited long enough ago to write them.
    SettingsDue,
}

pub enum Instruction {
    /// Files the program may read changed: build and show it again.
    ProgramChanged,
    /// Every queued write finished.
    Drained,
    /// Ask the user before going on. The folder leaves the document alone until it is released.
    Ask(Prompt),
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
            settings_dirty_since: None,
            read_only: false,
            gone: false,
            manifest: None,
            root: None,
            read: None,
            explorer: explorer::State::new(),
            opening: None,
            held: false,
            disk: None,
        };
        let generate = state.generate();
        let settings = state.persist();
        // A conflict means the folder has its own manifest, which stays as it is.
        let manifest = state.enqueue(manifest(), root_manifest(), Expect::Missing);
        let listing = state.list(None);

        (state, Task::batch([generate, settings, manifest, listing]))
    }

    /// Applies `message`, which arrived at `now`. Opening a file replaces `document`, and changes
    /// on disk reach it by the rules for unsaved changes, asking the user when they would lose
    /// some. `notice` receives what the user should know.
    pub fn update(
        &mut self,
        message: Message,
        now: Instant,
        document: &mut document::State,
        analyzer: &mut Analyzer,
        settings: &core::Settings,
        notice: &mut Option<String>,
    ) -> Action<Instruction, Message> {
        match message {
            Message::Watched(session, _)
            | Message::Wrote { session, .. }
            | Message::Read { session, .. }
            | Message::Listed { session, .. }
            | Message::Opened { session, .. }
                if session != self.session =>
            {
                Action::none()
            }
            Message::Watched(_, Watched::Scan(scan)) => {
                let changes = self.mirror.replace(&scan);
                changes
                    .iter()
                    .for_each(|change| self.explorer.changed(change));
                let (relevant, task, prompt) = self.apply(&changes, document, analyzer, notice);
                let first = !self.scanned;
                self.scanned = true;

                if first
                    && let Some(main) = self.settings.main.clone()
                    && let Some(text) = self.mirror.files().get(&main).cloned()
                {
                    self.show(document, main, &text, analyzer, settings);
                }

                Action {
                    instruction: prompt
                        .map(Instruction::Ask)
                        .or_else(|| (first || relevant).then_some(Instruction::ProgramChanged)),
                    task,
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            Message::Watched(_, Watched::Changes(changes)) => {
                // The explorer lists directories itself, so it follows changes the mirror drops.
                changes
                    .iter()
                    .for_each(|change| self.explorer.changed(change));
                if !self.scanned {
                    return Action::none();
                }
                let (relevant, task, prompt) = self.apply(&changes, document, analyzer, notice);
                Action {
                    instruction: prompt
                        .map(Instruction::Ask)
                        .or_else(|| relevant.then_some(Instruction::ProgramChanged)),
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
                let mut prompt = None;

                match result {
                    Ok(changes) => {
                        // The mirror leaves out bulin's own files, which the analyzer still reads.
                        if is_skipped(&path) {
                            analyzer.changed(&[(
                                path.clone(),
                                Effect::Changed,
                                Some(text.clone()),
                            )]);
                        }
                        document.saved(&path, &text);
                        if is_open(document, &path) {
                            self.disk = Some((path.clone(), Some(text)));
                        }
                        changes
                            .iter()
                            .for_each(|change| self.explorer.changed(change));
                        if self.scanned {
                            let applied = self.apply(&changes, document, analyzer, notice);
                            relevant = applied.0;
                            task = Task::batch([task, applied.1]);
                            prompt = applied.2;
                        }
                    }
                    // The folder has a manifest of its own.
                    Err(Error::Conflict { .. }) if path == manifest() => {}
                    Err(Error::Conflict { disk }) if is_open(document, &path) => {
                        let deleted = disk.is_none();
                        self.disk = Some((path.clone(), disk));
                        // While the user answers another question, the document stays dirty and
                        // is checked against storage once they have.
                        prompt = self.ask(Prompt::Conflict { path, deleted });
                    }
                    Err(Error::Conflict { disk }) => {
                        let what = if disk.is_none() {
                            "was deleted"
                        } else {
                            "changed"
                        };
                        *notice = Some(format!("{path} {what} on disk, so it wasn't saved."));
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

                let instruction = if let Some(prompt) = prompt {
                    Some(Instruction::Ask(prompt))
                } else if relevant {
                    Some(Instruction::ProgramChanged)
                } else {
                    self.queue.is_empty().then_some(Instruction::Drained)
                };
                Action { instruction, task }
            }
            Message::Read { path, result, .. } => {
                if !is_open(document, &path) {
                    return Action::none();
                }
                let prompt = match result {
                    Ok(text) => self.follow(&path, Some(&text), document, analyzer),
                    Err(Error::Missing) => self.follow(&path, None, document, analyzer),
                    Err(error) => {
                        *notice = Some(format!("Could not read {path}: {error}"));
                        None
                    }
                };
                Action {
                    instruction: prompt.map(Instruction::Ask),
                    task: Task::none(),
                }
            }
            Message::Explorer(message) => match self.explorer.update(message) {
                Some(explorer::Instruction::Expand(dir)) => Action::task(self.list(Some(dir))),
                Some(explorer::Instruction::Open(path)) => {
                    if is_open(document, &path) {
                        return Action::none();
                    }
                    if unsaved(document) {
                        let then = Pending::OpenDocument(path);
                        return Action {
                            instruction: self.ask(Prompt::Unsaved { then }).map(Instruction::Ask),
                            task: Task::none(),
                        };
                    }
                    Action::task(self.open(path, document, analyzer, settings))
                }
                Some(explorer::Instruction::SetMain(path)) => {
                    if let Err(error) = Module::of(self.root.as_ref(), &path) {
                        *notice = Some(format!("{path} can't be the main file: {error}"));
                        return Action::none();
                    }
                    self.settings.main = Some(path);
                    // Choosing a main file is a deliberate change, so it may replace settings
                    // that couldn't be read.
                    self.settings_locked = false;
                    Action::task(self.persist()).with_instruction(Instruction::ProgramChanged)
                }
                None => Action::none(),
            },
            Message::Listed { dir, result, .. } => {
                let entries = result.unwrap_or_else(|error| {
                    let name = dir
                        .as_ref()
                        .map_or_else(|| "the folder".to_owned(), Path::to_string);
                    *notice = Some(format!("Could not list {name}: {error}"));
                    Vec::new()
                });
                self.explorer.listed(dir, entries);
                Action::none()
            }
            Message::SettingsDue => match self.settings_dirty_since {
                Some(since) if now.saturating_duration_since(since) >= SETTLE => {
                    self.settings_dirty_since = None;
                    Action::task(self.persist())
                }
                Some(_) | None => Action::none(),
            },
            Message::Opened { path, result, .. } => {
                let discarded = match &self.opening {
                    Some((opening, discarded)) if *opening == path => *discarded,
                    Some(_) | None => return Action::none(),
                };
                self.opening = None;
                match result {
                    // The window stays usable while the file is read, so the user may have typed.
                    Ok(_) if !discarded && unsaved(document) => {
                        let then = Pending::OpenDocument(path);
                        Action {
                            instruction: self.ask(Prompt::Unsaved { then }).map(Instruction::Ask),
                            task: Task::none(),
                        }
                    }
                    Ok(text) => {
                        self.show(document, path, &text, analyzer, settings);
                        Action::none()
                    }
                    Err(error) => {
                        *notice = Some(format!("Could not open {path}: {error}"));
                        Action::none()
                    }
                }
            }
        }
    }

    /// The explorer, marking `document`'s file and the main file.
    pub fn view(&self, document: &document::State) -> Element<'_, Message> {
        let open = match document.location() {
            document::Location::Folder(path) => Some(path),
            document::Location::Untitled | document::Location::File(_) => None,
        };
        self.explorer
            .view(open, self.settings.main.as_ref())
            .map(Message::Explorer)
    }

    /// Watches the folder for changes, starting with a scan of its files, and checks for
    /// settings to write while edits are unwritten.
    pub fn subscription(&self) -> Subscription<Message> {
        let watch = watch(self.session, &self.handle)
            .map(|(session, watched)| Message::Watched(session, watched));
        // The tick's own instant isn't `now`'s type on the web, so `update` reads `now` instead.
        let due = if self.settings_dirty_since.is_some() {
            time::every(Duration::from_millis(250)).map(|_| Message::SettingsDue)
        } else {
            Subscription::none()
        };
        Subscription::batch([watch, due])
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

    /// Makes the folder file `path` the document, reading it first unless its text is known.
    /// Unsaved changes to the document are given up.
    pub fn open(
        &mut self,
        path: Path,
        document: &mut document::State,
        analyzer: &mut Analyzer,
        settings: &core::Settings,
    ) -> Task<Message> {
        // A queued write holds newer text than storage does.
        let text = self
            .queue
            .newest(&path)
            .or_else(|| self.mirror.files().get(&path).map(String::as_str))
            .map(str::to_owned);
        if let Some(text) = text {
            self.opening = None;
            self.show(document, path, &text, analyzer, settings);
            return Task::none();
        }

        self.opening = Some((path.clone(), unsaved(document)));
        let session = self.session;
        Task::perform(read(self.handle.clone(), path.clone()), move |result| {
            Message::Opened {
                session,
                path,
                result,
            }
        })
    }

    /// Keeps changes on disk from touching the document while the user answers a question.
    pub fn hold(&mut self) {
        self.held = true;
    }

    /// Lets changes on disk reach the document again once the user answered, checking it against
    /// what storage holds now, which may raise the next question.
    pub fn release(
        &mut self,
        document: &mut document::State,
        analyzer: &mut Analyzer,
    ) -> Action<Instruction, Message> {
        self.held = false;
        let document::Location::Folder(path) = document.location().clone() else {
            return Action::none();
        };
        let disk = self.on_disk(&path, document);
        Action {
            instruction: self
                .follow(&path, disk.as_deref(), document, analyzer)
                .map(Instruction::Ask),
            task: Task::none(),
        }
    }

    /// Replaces the document's text with what storage holds for the folder file `path` now, when
    /// it holds anything.
    pub fn reload(&self, path: &Path, document: &mut document::State, analyzer: &mut Analyzer) {
        if let Some(text) = self.on_disk(path, document) {
            document.reloaded(path, &text, analyzer);
        }
    }

    /// Keeps the document's text over what storage holds for the folder file `path` now, so the
    /// next save overwrites it.
    pub fn keep(&self, path: &Path, document: &mut document::State) {
        document.disk_changed(path, self.on_disk(path, document).as_deref());
    }

    /// Writes `text` to `path` whatever storage holds there, unless the folder is gone.
    pub fn overwrite(&mut self, path: Path, text: String) -> Task<Message> {
        if self.gone {
            return Task::none();
        }
        self.enqueue(path, text, Expect::Anything)
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

    /// Keeps the settings' uniforms and clock, and writes them and the uniforms package for their
    /// layout right away.
    pub fn layout_changed(&mut self, uniforms: &Uniforms, clock: Option<Clock>) -> Task<Message> {
        self.settings.uniforms = uniforms.clone();
        self.settings.clock = clock;
        // Adding or removing a uniform is deliberate, so it may replace settings that couldn't be
        // read.
        self.settings_locked = false;
        self.settings_dirty_since = None;
        Task::batch([self.generate(), self.persist()])
    }

    /// Keeps the uniforms and clock the user edited at `now`, and writes the settings once a
    /// second passes without another edit, so dragging a slider doesn't write on every move.
    pub fn edited(&mut self, uniforms: &Uniforms, clock: Option<Clock>, now: Instant) {
        self.settings.uniforms = uniforms.clone();
        self.settings.clock = clock;
        // An edit is deliberate, so it may replace settings that couldn't be read.
        self.settings_locked = false;
        self.settings_dirty_since = Some(now);
    }

    /// Keeps the time on the running clock for the next settings write, without writing.
    pub fn ticked(&mut self, clock: Option<Clock>) {
        self.settings.clock = clock;
    }

    /// Keeps the uniforms and clock as the folder is left and writes the settings, so the folder
    /// reopens with them. Settings that couldn't be read stay untouched, since leaving isn't a
    /// deliberate change.
    pub fn leave(&mut self, uniforms: &Uniforms, clock: Option<Clock>) -> Task<Message> {
        self.settings.uniforms = uniforms.clone();
        self.settings.clock = clock;
        self.settings_dirty_since = None;
        self.persist()
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

    /// Applies `changes` to the mirror, the root manifest, the document and the analyzer, returning
    /// whether the program may read a changed file, the reads and writes they call for, and what
    /// to ask the user about the document.
    fn apply(
        &mut self,
        changes: &[Change],
        document: &mut document::State,
        analyzer: &mut Analyzer,
        notice: &mut Option<String>,
    ) -> (bool, Task<Message>, Option<Prompt>) {
        let generated = generated("").map(|(path, _)| path);
        let mut relevant = false;
        let mut prompt = None;
        let mut regenerate = false;
        let mut tasks = Vec::new();
        let mut files = Vec::new();

        for change in changes {
            for (path, effect) in self.mirror.apply(change) {
                relevant |= self.is_relevant(&path);
                let text = match effect {
                    Effect::Created | Effect::Changed => self.mirror.files().get(&path).cloned(),
                    Effect::Removed => None,
                };
                files.push((path, effect, text));
            }

            let document::Location::Folder(open) = document.location().clone() else {
                continue;
            };
            match change {
                Change::Changed { path, text } if *path == open => match text {
                    Some(text) => {
                        let asked = self.follow(&open, Some(text.as_str()), document, analyzer);
                        prompt = prompt.or(asked);
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
                    let asked = self.follow(&open, None, document, analyzer);
                    prompt = prompt.or(asked);
                }
                Change::Changed { .. } | Change::Removed(_) => {}
            }

            regenerate |= matches!(change, Change::Removed(path) if generated.contains(path));
        }
        analyzer.changed(&files);

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
        (relevant, Task::batch(tasks), prompt)
    }

    /// Brings the open folder file `path` in line with storage now holding `text`, `None` when
    /// the file is gone, unless the document is held. Returns what to ask the user when that
    /// would lose unsaved changes or the file is gone.
    fn follow(
        &mut self,
        path: &Path,
        text: Option<&str>,
        document: &mut document::State,
        analyzer: &mut Analyzer,
    ) -> Option<Prompt> {
        self.disk = Some((path.clone(), text.map(str::to_owned)));
        if self.held {
            return None;
        }
        let Some(text) = text else {
            // A file deleted earlier was asked about then.
            document.base()?;
            document.disk_changed(path, None);
            return self.ask(Prompt::ChangedOnDisk {
                path: path.clone(),
                deleted: true,
            });
        };

        // An echo's completion updates the document, and a base equal to `text` is up to date.
        if self.queue.is_echo(path, text)
            || document.base().is_some_and(|base| same_text(base, text))
        {
            return None;
        }
        if same_text(text, &document.content()) {
            document.disk_changed(path, Some(text));
            None
        } else if !document.is_dirty() {
            document.reloaded(path, text, analyzer);
            None
        } else {
            self.ask(Prompt::ChangedOnDisk {
                path: path.clone(),
                deleted: false,
            })
        }
    }

    /// `prompt`, holding the document until the user answers, unless they answer another question
    /// already.
    fn ask(&mut self, prompt: Prompt) -> Option<Prompt> {
        if self.held {
            return None;
        }
        self.held = true;
        Some(prompt)
    }

    /// What storage holds for the folder file `path`, which is `document`: the last report since
    /// it opened, or else its `base`.
    fn on_disk(&self, path: &Path, document: &document::State) -> Option<String> {
        match &self.disk {
            Some((at, text)) if at == path => text.clone(),
            Some(_) | None => document.base().map(str::to_owned),
        }
    }

    /// Replaces `document` with the folder file `path` holding `text`. A new editor, unlike loading
    /// the text into the old one, leaves nothing for Undo to bring back from the previous file.
    fn show(
        &mut self,
        document: &mut document::State,
        path: Path,
        text: &str,
        analyzer: &mut Analyzer,
        settings: &core::Settings,
    ) {
        self.disk = None;
        document.detach(analyzer);
        *document = document::State::folder(path, text, settings);
        document.attach(analyzer);
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

    /// Lists the directory `dir`, `None` being the folder itself, for the explorer.
    fn list(&self, dir: Option<Path>) -> Task<Message> {
        let session = self.session;
        Task::perform(scan_dir(self.handle.clone(), dir.clone()), move |result| {
            Message::Listed {
                session,
                dir,
                result,
            }
        })
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

/// Whether `document` is a folder file with unsaved changes, which opening another file or leaving
/// the folder would lose.
pub fn unsaved(document: &document::State) -> bool {
    matches!(document.location(), document::Location::Folder(_)) && document.is_dirty()
}

/// Whether `document` is the folder file `path`.
fn is_open(document: &document::State, path: &Path) -> bool {
    matches!(document.location(), document::Location::Folder(open) if open == path)
}

/// How long the settings stay unwritten after the last edit.
const SETTLE: Duration = Duration::from_secs(1);

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
    use crate::analyzer::Workspace;
    use crate::core::folder::MAX_TEXT;
    use crate::core::uniform::Type;

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
            let (analyzer, events) = Analyzer::start(Workspace::Scratch);
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
            self.update_at(message, Instant::now())
        }

        fn update_at(&mut self, message: Message, now: Instant) -> Option<Instruction> {
            self.state
                .update(
                    message,
                    now,
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

        fn explore(&mut self, message: explorer::Message) -> Option<Instruction> {
            self.update(Message::Explorer(message))
        }

        /// Opens `file`, which isn't in the mirror, with `result` as what reading it gave.
        fn open_read(&mut self, file: &str, result: Result<String, Error>) {
            let _ = self.explore(explorer::Message::Open(path(file)));
            let _ = self.update(Message::Opened {
                session: 1,
                path: path(file),
                result,
            });
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

    /// Releases the document after the user answered, returning what the folder asks next.
    fn release(open: &mut Open) -> Option<Instruction> {
        open.state
            .release(&mut open.document, &mut open.analyzer)
            .instruction
    }

    #[test]
    fn a_dirty_document_asks_and_keeps_its_text() {
        let mut open = Open::new(&[("main.wesl", "a")]);
        open.type_text("x");

        let instruction = open.changes(vec![changed("main.wesl", Some("b"))]);

        assert!(matches!(
            instruction,
            Some(Instruction::Ask(Prompt::ChangedOnDisk {
                deleted: false,
                ..
            }))
        ));
        assert_eq!(open.document.content(), "xa");
        assert_eq!(open.document.base(), Some("a"));
        assert!(open.document.is_dirty());
        assert_eq!(open.notice, None);

        open.state.keep(&path("main.wesl"), &mut open.document);
        assert_eq!(open.document.base(), Some("b"));
        assert!(release(&mut open).is_none());

        let instruction = open.changes(vec![Change::Removed(path("main.wesl"))]);
        assert!(matches!(
            instruction,
            Some(Instruction::Ask(Prompt::ChangedOnDisk {
                deleted: true,
                ..
            }))
        ));
        assert_eq!(open.document.base(), None);
        assert_eq!(open.document.content(), "xa");
    }

    #[test]
    fn deleting_a_clean_document_asks_and_keeps_it() {
        let mut open = Open::new(&[("main.wesl", "a")]);

        let instruction = open.changes(vec![Change::Removed(path("main.wesl"))]);

        assert!(matches!(
            instruction,
            Some(Instruction::Ask(Prompt::ChangedOnDisk {
                deleted: true,
                ..
            }))
        ));
        assert_eq!(open.document.content(), "a");
        assert!(open.document.is_dirty());
    }

    #[test]
    fn a_conflict_on_save_asks_and_keeps_base() {
        let mut open = Open::new(&[("main.wesl", "a")]);
        open.type_text("x");
        let text = open.save();

        let disk = Some("disk".to_owned());
        let instruction = open.update(Message::Wrote {
            session: 1,
            path: path("main.wesl"),
            text,
            result: Err(Error::Conflict { disk }),
        });

        assert!(matches!(
            instruction,
            Some(Instruction::Ask(Prompt::Conflict { deleted: false, .. }))
        ));
        assert_eq!(open.document.base(), Some("a"));
        assert!(open.document.is_dirty());
        assert_eq!(open.notice, None);
        assert!(open.state.is_idle());

        open.state
            .reload(&path("main.wesl"), &mut open.document, &mut open.analyzer);
        assert_eq!(
            open.document.content(),
            "disk",
            "Reload takes what storage holds"
        );
        assert!(!open.document.is_dirty());
    }

    #[test]
    fn changes_while_held_wait_for_the_release() {
        let mut open = Open::new(&[("main.wesl", "a")]);
        open.type_text("x");
        open.state.hold();

        for text in ["b", "c"] {
            let instruction = open.changes(vec![changed("main.wesl", Some(text))]);
            assert!(!matches!(instruction, Some(Instruction::Ask(_))));
        }
        assert_eq!(open.state.mirror.files()[&path("main.wesl")], "c");
        assert_eq!(open.document.base(), Some("a"));

        assert!(matches!(
            release(&mut open),
            Some(Instruction::Ask(Prompt::ChangedOnDisk {
                deleted: false,
                ..
            }))
        ));
        open.state
            .reload(&path("main.wesl"), &mut open.document, &mut open.analyzer);
        assert_eq!(open.document.content(), "c", "Reload takes the newest text");
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
        let (mut analyzer, _events) = Analyzer::start(Workspace::Scratch);
        let mut document = document::State::new("", &core::Settings::default());
        let mut notice = None;
        let mut update = |state: &mut State, message| {
            state
                .update(
                    message,
                    Instant::now(),
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
    fn set_main_rejects_an_invalid_module_name() {
        let mut open = Open::new(&[("main.wesl", "a"), ("my-shader.wesl", "m")]);

        let instruction = open.explore(explorer::Message::SetMain(path("my-shader.wesl")));

        assert!(instruction.is_none());
        assert!(
            open.notice.as_deref().unwrap().contains("`my-shader`"),
            "{:?}",
            open.notice
        );
        assert_eq!(open.state.settings.main, Some(path("main.wesl")));
        assert!(open.state.is_idle());
    }

    #[test]
    fn set_main_writes_settings_and_recompiles() {
        let mut open = Open::new(&[("main.wesl", "a"), ("other.wesl", "o")]);

        let instruction = open.explore(explorer::Message::SetMain(path("other.wesl")));

        assert!(matches!(instruction, Some(Instruction::ProgramChanged)));
        let write = open.state.in_flight().unwrap();
        assert_eq!(write.path, path(SETTINGS));
        assert_eq!(
            Settings::from_json(&write.text).unwrap().main,
            Some(path("other.wesl"))
        );
    }

    #[test]
    fn opening_while_dirty_asks() {
        let mut open = Open::new(&[("main.wesl", "a"), ("other.wesl", "o")]);
        open.type_text("x");

        let instruction = open.explore(explorer::Message::Open(path("other.wesl")));
        assert!(matches!(
            instruction,
            Some(Instruction::Ask(Prompt::Unsaved {
                then: Pending::OpenDocument(file),
            })) if file == path("other.wesl")
        ));
        assert_eq!(open.document.content(), "xa");
        assert!(release(&mut open).is_none());

        let _ = open.state.open(
            path("other.wesl"),
            &mut open.document,
            &mut open.analyzer,
            &core::Settings::default(),
        );
        assert_eq!(open.document.content(), "o", "discarding opens it anyway");
    }

    #[test]
    fn typing_while_a_file_is_read_asks_before_it_opens() {
        let mut open = Open::new(&[("main.wesl", "a")]);

        let _ = open.explore(explorer::Message::Open(path("notes.md")));
        open.type_text("x");
        let instruction = open.update(Message::Opened {
            session: 1,
            path: path("notes.md"),
            result: Ok("notes".to_owned()),
        });

        assert!(matches!(
            instruction,
            Some(Instruction::Ask(Prompt::Unsaved { .. }))
        ));
        assert_eq!(open.document.content(), "xa");

        let _ = release(&mut open);
        let _ = open.state.open(
            path("notes.md"),
            &mut open.document,
            &mut open.analyzer,
            &core::Settings::default(),
        );
        let _ = open.update(Message::Opened {
            session: 1,
            path: path("notes.md"),
            result: Ok("notes".to_owned()),
        });
        assert_eq!(
            open.document.content(),
            "notes",
            "discarding opens it anyway"
        );
    }

    #[test]
    fn opening_a_file_replaces_the_document() {
        let mut open = Open::new(&[("main.wesl", "a"), ("other.wesl", "o")]);

        let _ = open.explore(explorer::Message::Open(path("other.wesl")));
        assert_eq!(open.document.content(), "o");

        open.open_read("notes.md", Ok("notes".to_owned()));
        assert_eq!(open.document.content(), "notes");
        assert!(matches!(
            open.document.location(),
            document::Location::Folder(file) if *file == path("notes.md")
        ));

        let _ = open.explore(explorer::Message::Open(path("late.md")));
        let _ = open.explore(explorer::Message::Open(path("other.wesl")));
        let _ = open.update(Message::Opened {
            session: 1,
            path: path("late.md"),
            result: Ok("late".to_owned()),
        });
        assert_eq!(
            open.document.content(),
            "o",
            "a later open supersedes a read"
        );
        assert_eq!(open.notice, None);
    }

    #[test]
    fn a_large_or_binary_file_doesnt_open() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("big.wesl"), vec![b'a'; MAX_TEXT + 1]).unwrap();
        std::fs::write(dir.path().join("binary.wesl"), [0xff, 0xfe]).unwrap();
        let disk = Handle::new(dir.path().to_owned());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut open = Open::new(&[("main.wesl", "a")]);

        for (file, why) in [("big.wesl", "1 MiB"), ("binary.wesl", "UTF-8")] {
            let result = runtime.block_on(read(disk.clone(), path(file)));
            open.open_read(file, result);

            assert_eq!(open.document.content(), "a");
            let notice = open.notice.take().unwrap();
            assert!(
                notice.starts_with(&format!("Could not open {file}")),
                "{notice}"
            );
            assert!(notice.contains(why), "{notice}");
        }
    }

    #[test]
    fn bulin_documents_are_read_only() {
        let mut open = Open::new(&[("main.wesl", "a")]);
        let [_, (bulin, source), _, (uniforms, _)] = generated("old");

        open.open_read(bulin.as_str(), Ok(source.clone()));
        open.type_text("x");
        assert_eq!(open.document.content(), source);
        assert!(!open.document.is_dirty());
        let action = open.document.update(
            document::Message::SaveFile,
            Instant::now(),
            &mut open.analyzer,
            &core::Settings::default(),
        );
        assert!(action.instruction.is_none());

        open.open_read(uniforms.as_str(), Ok("old".to_owned()));
        open.wrote(
            uniforms.as_str(),
            "new",
            Ok(vec![changed(uniforms.as_str(), Some("new"))]),
        );
        assert_eq!(open.document.content(), "new", "bulin's writes reach it");
        assert!(!open.document.is_dirty());
    }

    fn radius(value: f32) -> Uniforms {
        let mut uniforms = Uniforms::default();
        uniforms.add("radius", Type::Float(value)).unwrap();
        uniforms
    }

    /// The settings the write in flight carries, when it writes them.
    fn written_settings(state: &State) -> Option<Settings> {
        let write = state.in_flight()?;
        (write.path == path(SETTINGS)).then(|| Settings::from_json(&write.text).unwrap())
    }

    #[test]
    fn settings_are_written_one_second_after_the_last_edit() {
        let mut open = Open::new(&[("main.wesl", "a")]);
        let t0 = Instant::now();
        let at = |seconds: f32| t0 + Duration::from_secs_f32(seconds);

        open.state.edited(&radius(0.5), None, t0);
        let _ = open.update_at(Message::SettingsDue, at(0.5));
        assert!(open.state.is_idle());

        open.state.edited(&radius(0.7), None, at(0.8));
        let _ = open.update_at(Message::SettingsDue, at(1.5));
        assert!(open.state.is_idle(), "the second edit restarts the wait");

        let _ = open.update_at(Message::SettingsDue, at(1.9));
        assert_eq!(written_settings(&open.state).unwrap().uniforms, radius(0.7));
        assert_eq!(open.state.queue.writes.len(), 1);
        assert_eq!(open.state.settings_dirty_since, None);
    }

    #[test]
    fn ticks_dont_persist() {
        let mut open = Open::new(&[("main.wesl", "a")]);
        let mut clock = Clock::started();

        for _ in 0..100 {
            clock.advance(Duration::from_millis(10));
            open.state.ticked(Some(clock));
        }

        assert_eq!(open.state.settings_dirty_since, None);
        assert!(open.state.is_idle());
        assert_eq!(open.state.settings.clock, Some(clock));
    }

    #[test]
    fn a_layout_change_writes_settings_right_away() {
        let mut open = Open::new(&[("main.wesl", "a")]);
        open.state.edited(&radius(0.5), None, Instant::now());

        let _ = open
            .state
            .layout_changed(&radius(0.5), Some(Clock::started()));

        let queued = open
            .state
            .queue
            .writes
            .iter()
            .map(|write| write.path.as_str())
            .collect::<Vec<_>>();
        assert!(queued.contains(&SETTINGS), "{queued:?}");
        let settings = open.state.queue.newest(&path(SETTINGS)).unwrap();
        let settings = Settings::from_json(settings).unwrap();
        assert_eq!(settings.uniforms, radius(0.5));
        assert_eq!(settings.clock, Some(Clock::started()));
        assert_eq!(open.state.settings_dirty_since, None);
    }

    #[test]
    fn leaving_writes_the_elapsed_clock() {
        let mut open = Open::new(&[("main.wesl", "a")]);
        let mut clock = Clock::started();
        for _ in 0..50 {
            clock.advance(Duration::from_millis(100));
            open.state.ticked(Some(clock));
        }
        assert!(open.state.is_idle());

        let _ = open.state.leave(&Uniforms::default(), Some(clock));

        let settings = written_settings(&open.state).unwrap();
        assert_eq!(settings.clock.unwrap().seconds(), clock.seconds());
        assert!(clock.seconds() > 4.9);
    }

    #[test]
    fn locked_settings_are_not_overwritten_on_leave() {
        let (mut state, _) = State::new(handle(), 1, Settings::default(), true);
        while state.queue.complete(true).is_some() {}

        let _ = state.leave(&radius(0.5), None);
        assert!(state.is_idle(), "leaving isn't a deliberate change");

        state.edited(&radius(0.7), None, Instant::now());
        let _ = state.leave(&radius(0.7), None);
        assert_eq!(written_settings(&state).unwrap().uniforms, radius(0.7));
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
