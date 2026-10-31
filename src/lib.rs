pub use bulin_core as core;

mod action;
mod analyzer;
mod control;
mod document;
mod explorer;
mod file;
mod folder;
mod layout;
mod menu;
mod prompt;
mod settings;
mod uniforms;
mod viewer;

use iced::keyboard::key;
use iced::task;
use iced::time::Instant;
use iced::widget::{
    button, center, column, container, mouse_area, opaque, operation, row, stack, text,
};
use iced::{
    Center, Element, Event, Fill, Font, Function, Length, Subscription, Task, Theme, color, event,
    keyboard, window,
};
use matcha_iced::lsp;

use std::sync::Arc;

use crate::core::folder::same_text;
use crate::prompt::{Answer, Prompt};

/// matcha's default id for the editor, which takes focus back when a modal closes.
const EDITOR: &str = "matcha-editor";

pub struct Application {
    /// The running language server. It is `None` only inside [`Application::restart`], so the old
    /// server is gone before the next starts.
    analyzer: Option<analyzer::Analyzer>,
    /// Numbers the running analyzer, so events from an earlier one are dropped.
    generation: u64,
    /// Aborts the running analyzer's event stream when replaced.
    analyzer_events: task::Handle,
    document: document::State,
    uniforms: uniforms::State,
    viewer: viewer::State,
    layout: layout::State,
    mode: Mode,
    opener: folder::opener::Opener,
    /// The question the user is answering. Nothing else that would replace work runs until they
    /// do.
    prompt: Option<Prompt>,
    /// What runs once the saves it waits for are done.
    draining: Option<Drain>,
    /// Counts folder openings and analyzer starts, numbering each so results meant for an earlier
    /// one are dropped.
    sessions: u64,
    is_loading: bool,
    modal: Option<Modal>,
    settings: core::Settings,
    /// Whether the settings are being written.
    saving_settings: bool,
    /// Whether the settings changed during the write, so they need writing again.
    settings_stale: bool,
    /// Whether the stored settings couldn't be read, so they are backed up before the first
    /// write replaces them.
    settings_unreadable: bool,
    /// Why the last action failed or what it found, until the next one.
    notice: Option<String>,
}

/// What the layer over the content shows.
enum Modal {
    Menu,
    Settings(settings::State),
}

/// What the editor works on: a project, or a folder of files.
enum Mode {
    /// A project, saved in `file` once it has one. `text` is its shader as last loaded or saved,
    /// and `dirty` says its uniforms or clock were edited since.
    Project {
        file: Option<file::Name>,
        dirty: bool,
        text: String,
    },
    Folder(Box<folder::State>),
}

/// What the user asked for that waits for an answer or for saves to finish.
enum Pending {
    OpenFolder(folder::Handle),
    /// A project file and its text, which parses.
    OpenProject(file::Name, Arc<String>),
    CloseFolder,
    /// A folder file to make the document.
    OpenDocument(core::folder::Path),
    NewProject,
    OpenFile,
    NewFile,
    Quit,
}

/// `then`, waiting for the save `step`.
struct Drain {
    then: Pending,
    step: Step,
}

/// Which save a [`Drain`] waits for.
enum Step {
    /// The document's file, picked in a dialog.
    File,
    /// The project.
    Project,
    /// The folder's queued writes.
    Queue,
}

/// Why the app can't take another file action now.
enum Busy {
    /// A question waits for an answer.
    Prompt,
    /// A dialog or the library is open.
    Dialog,
    /// The folder's writes are finishing before it is left.
    Draining,
}

#[derive(Debug, Clone)]
pub enum Message {
    Document(document::Message),
    Uniforms(uniforms::Message),
    Layout(layout::Message),
    Folder(folder::Message),
    Opener(folder::opener::Message),
    Prompt(Answer),
    ShowMenu,
    CloseModal,
    Escape,
    Menu(menu::Message),
    Settings(settings::Message),
    OpenProject,
    OpenFolder,
    CloseFolder,
    FolderSettings {
        session: u64,
        handle: folder::Handle,
        result: Result<String, folder::Error>,
    },
    NewProject,
    SaveProject,
    SaveProjectAs,
    /// Where the project was saved, with the shader and uniforms saved there.
    ProjectSaved(Result<(file::Name, String, core::Uniforms), file::Error>),
    /// The user asked to close the window.
    CloseRequested,
    /// An event of the analyzer with this generation.
    Analyzer(u64, lsp::client::Event),
    /// A key no widget took, which may be a shortcut bulin handles.
    KeyPressed {
        key: keyboard::Key,
        physical: keyboard::key::Physical,
        modifiers: keyboard::Modifiers,
    },
    SettingsSaved(Result<(), settings::storage::Error>),
}

impl Pending {
    /// Whether this leaves the folder, when one is open.
    fn leaves(&self) -> bool {
        match self {
            Pending::OpenFolder(_)
            | Pending::OpenProject(..)
            | Pending::CloseFolder
            | Pending::Quit => true,
            Pending::OpenDocument(_)
            | Pending::NewProject
            | Pending::OpenFile
            | Pending::NewFile => false,
        }
    }
}

impl Application {
    pub fn new() -> (Self, Task<Message>) {
        match settings::storage::load() {
            Ok(settings) => Self::with_settings(settings.unwrap_or_default()),
            Err(error) => {
                let (mut application, task) = Self::with_settings(core::Settings::default());
                application.notice = Some(format!("Could not load the settings: {error}"));
                application.settings_unreadable = true;
                (application, task)
            }
        }
    }

    /// Starts on an untitled project with `settings` rather than the stored ones.
    fn with_settings(settings: core::Settings) -> (Self, Task<Message>) {
        let (mut analyzer, events) = analyzer::Analyzer::start(analyzer::Workspace::Scratch);
        let (events, analyzer_events) = Task::run(events, Message::Analyzer.with(0)).abortable();
        let project = core::Project::untitled();
        let mut document = document::State::new(&project.shader, &settings);
        document.attach(&mut analyzer);
        let uniforms = uniforms::State::new(project.uniforms, project.clock);
        let program = core::Program::project(
            document.content(),
            uniforms.uniforms().clone(),
            uniforms.time(),
        );
        analyzer.set_uniforms(&program.uniforms_package());
        let viewer = viewer::State::new(&program);

        let application = Self {
            analyzer: Some(analyzer),
            generation: 0,
            analyzer_events: analyzer_events.abort_on_drop(),
            document,
            uniforms,
            viewer,
            layout: layout::State::new(),
            mode: Mode::Project {
                file: None,
                dirty: false,
                text: project.shader,
            },
            opener: folder::opener::Opener::default(),
            prompt: None,
            draining: None,
            sessions: 0,
            is_loading: false,
            modal: None,
            settings,
            saving_settings: false,
            settings_stale: false,
            settings_unreadable: false,
            notice: None,
        };

        (application, events)
    }

    pub fn update(&mut self, message: Message, now: Instant) -> Task<Message> {
        let task = self.react(message, now);
        file::warn_on_unload(self.is_dirty());
        task
    }

    /// Applies `message`, the part of [`Application::update`] each message differs in.
    fn react(&mut self, message: Message, now: Instant) -> Task<Message> {
        match message {
            Message::Document(message) => {
                // Replacing the document waits for an answer when something is unsaved.
                if matches!(message, document::Message::NewFile) {
                    return self.ask(Pending::NewFile, now);
                }
                if matches!(message, document::Message::OpenFile) {
                    return self.ask(Pending::OpenFile, now);
                }
                // A document dialog answering after a project switch would land in the new project.
                let file_action = matches!(
                    message,
                    document::Message::SaveFile | document::Message::SaveFileAs
                );
                if file_action && self.refuse() {
                    return Task::none();
                }
                // Only folder files save in a folder: the placeholder before the main file opens
                // has nowhere to go.
                if matches!(message, document::Message::SaveFile)
                    && matches!(self.mode, Mode::Folder(_))
                    && !matches!(self.document.location(), document::Location::Folder(_))
                {
                    return Task::none();
                }
                let action =
                    self.document
                        .update(message, now, running(&mut self.analyzer), &self.settings);
                let task = action.task.map(Message::Document);
                match action.instruction {
                    Some(document::Instruction::Changed) => self.program_changed(),
                    Some(document::Instruction::Save { path, text, base }) => {
                        if let Mode::Folder(state) = &mut self.mode {
                            let save = state.save(path, text, base, &mut self.notice);
                            return Task::batch([task, save.map(Message::Folder)]);
                        }
                    }
                    Some(document::Instruction::FileSaved(result)) => {
                        // Only a Save answered at a prompt waits for the file; a failed or
                        // cancelled save leaves what waits undone.
                        if let Some(drain) = self
                            .draining
                            .take_if(|drain| matches!(drain.step, Step::File))
                            && result.is_ok()
                        {
                            let next = self.save_from(drain.then, Step::Project, now);
                            return Task::batch([task, next]);
                        }
                    }
                    None => {}
                }
                task
            }
            Message::Uniforms(message) => {
                let action = self.uniforms.update(message);
                let task = action.task.map(Message::Uniforms);
                match action.instruction {
                    Some(uniforms::Instruction::Edited) => {
                        self.viewer.set_values(core::program::bytes(
                            self.uniforms.time(),
                            self.uniforms.uniforms(),
                        ));
                        match &mut self.mode {
                            Mode::Folder(state) => {
                                state.edited(self.uniforms.uniforms(), self.uniforms.clock(), now)
                            }
                            Mode::Project { dirty, .. } => *dirty = true,
                        }
                        task
                    }
                    Some(uniforms::Instruction::Ticked) => {
                        self.viewer.set_values(core::program::bytes(
                            self.uniforms.time(),
                            self.uniforms.uniforms(),
                        ));
                        if let Mode::Folder(state) = &mut self.mode {
                            state.ticked(self.uniforms.clock());
                        }
                        task
                    }
                    Some(uniforms::Instruction::Layout) => {
                        self.program_changed();
                        match &mut self.mode {
                            Mode::Folder(state) => Task::batch([
                                task,
                                state
                                    .layout_changed(self.uniforms.uniforms(), self.uniforms.clock())
                                    .map(Message::Folder),
                            ]),
                            Mode::Project { dirty, .. } => {
                                *dirty = true;
                                task
                            }
                        }
                    }
                    None => task,
                }
            }
            Message::Layout(message) => {
                self.layout.update(message);
                Task::none()
            }
            Message::Folder(message) => {
                let Mode::Folder(state) = &mut self.mode else {
                    return Task::none();
                };
                let mut notice = None;
                let action = state.update(
                    message,
                    now,
                    &mut self.document,
                    running(&mut self.analyzer),
                    &self.settings,
                    &mut notice,
                );
                let task = action.task.map(Message::Folder);
                match action.instruction {
                    Some(folder::Instruction::ProgramChanged) => self.program_changed(),
                    // The change that asks may be one the program reads.
                    Some(folder::Instruction::Ask(prompt)) => {
                        self.program_changed();
                        self.raise(prompt);
                    }
                    Some(folder::Instruction::Drained) | None => {}
                }
                if notice.is_some() {
                    self.notice = notice;
                    // A notice while the writes finish tells of one that failed, so what waits
                    // for them doesn't run.
                    self.draining = None;
                }
                // A completion can finish the queue whatever else it reports.
                Task::batch([task, self.drain(now)])
            }
            Message::Opener(message) => {
                let action = self.opener.update(message);
                let task = action.task.map(Message::Opener);
                let then = match action.instruction {
                    Some(folder::opener::Instruction::OpenFolder(handle)) => {
                        Some(Pending::OpenFolder(handle))
                    }
                    Some(folder::opener::Instruction::OpenProject(name, text)) => {
                        match core::Project::from_json(&text) {
                            Ok(_) => Some(Pending::OpenProject(name, text)),
                            Err(error) => {
                                self.notice = Some(format!("Could not open the project: {error}"));
                                None
                            }
                        }
                    }
                    Some(folder::opener::Instruction::Failed(notice)) => {
                        self.notice = Some(notice);
                        None
                    }
                    None => None,
                };
                let Some(then) = then else {
                    return task;
                };
                // The window stays usable while the user picks, so what is unsaved may have
                // changed.
                Task::batch([task, self.ask(then, now)])
            }
            Message::Prompt(answer) => {
                let Some(prompt) = self.prompt.take() else {
                    return Task::none();
                };
                let task = match (prompt, answer) {
                    (Prompt::Unsaved { then }, Answer::Save) => match &mut self.mode {
                        Mode::Folder(state) => {
                            // Straight to the document: the prompt would refuse its save.
                            let action = self.document.update(
                                document::Message::SaveFile,
                                now,
                                running(&mut self.analyzer),
                                &self.settings,
                            );
                            let mut tasks = vec![action.task.map(Message::Document)];
                            let mut notice = None;
                            if let Some(document::Instruction::Save { path, text, base }) =
                                action.instruction
                            {
                                tasks.push(
                                    state
                                        .save(path, text, base, &mut notice)
                                        .map(Message::Folder),
                                );
                            }
                            if notice.is_some() {
                                // The save was refused, so what waits for it doesn't run.
                                self.notice = notice;
                            } else {
                                if then.leaves() {
                                    tasks.push(
                                        state
                                            .leave(self.uniforms.uniforms(), self.uniforms.clock())
                                            .map(Message::Folder),
                                    );
                                }
                                self.draining = Some(Drain {
                                    then,
                                    step: Step::Queue,
                                });
                                tasks.push(self.drain(now));
                            }
                            Task::batch(tasks)
                        }
                        Mode::Project { .. } => self.save_from(then, Step::File, now),
                    },
                    (Prompt::Unsaved { then }, Answer::Discard) => self.start(then, now),
                    (
                        Prompt::Unsaved { .. },
                        Answer::Cancel
                        | Answer::Reload
                        | Answer::KeepMine
                        | Answer::Overwrite
                        | Answer::Delete,
                    ) => Task::none(),
                    (
                        Prompt::ChangedOnDisk { path, .. } | Prompt::Conflict { path, .. },
                        Answer::Reload,
                    ) => {
                        if let Mode::Folder(state) = &mut self.mode {
                            state.reload(&path, &mut self.document, running(&mut self.analyzer));
                        }
                        Task::none()
                    }
                    // Cancel is the choice that loses nothing.
                    (Prompt::ChangedOnDisk { path, .. }, Answer::KeepMine | Answer::Cancel) => {
                        if let Mode::Folder(state) = &mut self.mode {
                            state.keep(&path, &mut self.document);
                        }
                        Task::none()
                    }
                    (
                        Prompt::ChangedOnDisk { .. },
                        Answer::Save | Answer::Discard | Answer::Overwrite | Answer::Delete,
                    ) => Task::none(),
                    // What the editor holds now, typed after the question too.
                    (Prompt::Conflict { path, .. }, Answer::Overwrite) => match &mut self.mode {
                        Mode::Folder(state) => {
                            // The user has seen what storage holds, so it isn't asked about again.
                            state.keep(&path, &mut self.document);
                            state
                                .overwrite(path, self.document.content())
                                .map(Message::Folder)
                        }
                        Mode::Project { .. } => Task::none(),
                    },
                    (
                        Prompt::Conflict { .. },
                        Answer::Cancel
                        | Answer::Save
                        | Answer::Discard
                        | Answer::KeepMine
                        | Answer::Delete,
                    ) => Task::none(),
                    (Prompt::Delete { path }, Answer::Delete) => match &mut self.mode {
                        Mode::Folder(state) => state.delete(path).map(Message::Folder),
                        Mode::Project { .. } => Task::none(),
                    },
                    (
                        Prompt::Delete { .. },
                        Answer::Cancel
                        | Answer::Save
                        | Answer::Discard
                        | Answer::Reload
                        | Answer::KeepMine
                        | Answer::Overwrite,
                    ) => Task::none(),
                };

                // The document is checked against what changed on disk while the user answered.
                let Mode::Folder(state) = &mut self.mode else {
                    return task;
                };
                let action = state.release(&mut self.document, running(&mut self.analyzer));
                match action.instruction {
                    Some(folder::Instruction::Ask(prompt)) => self.raise(prompt),
                    Some(folder::Instruction::ProgramChanged) => self.program_changed(),
                    Some(folder::Instruction::Drained) | None => {}
                }
                Task::batch([task, action.task.map(Message::Folder)])
            }
            Message::ShowMenu => {
                self.modal = Some(Modal::Menu);
                // The editor would otherwise keep taking keys behind the modal.
                iced::advanced::widget::operate(
                    iced::advanced::widget::operation::focusable::unfocus(),
                )
            }
            Message::CloseModal => match self.modal.take() {
                Some(_) => operation::focus(EDITOR),
                None => Task::none(),
            },
            Message::Escape => match self.busy() {
                Some(Busy::Prompt) => self.react(Message::Prompt(Answer::Cancel), now),
                Some(Busy::Dialog) if self.opener.is_busy() => {
                    self.opener.close();
                    Task::none()
                }
                Some(Busy::Dialog | Busy::Draining) | None => self.react(Message::CloseModal, now),
            },
            Message::Menu(message) => {
                self.modal = None;
                let message = match message {
                    menu::Message::Settings => {
                        self.modal = Some(Modal::Settings(settings::State::new()));
                        return Task::none();
                    }
                    // The find bar takes focus as it opens.
                    menu::Message::Find => {
                        return self.react(
                            Message::Document(document::Message::Editor(
                                matcha_iced::Event::OpenFind,
                            )),
                            now,
                        );
                    }
                    menu::Message::Replace => {
                        return self.react(
                            Message::Document(document::Message::Editor(
                                matcha_iced::Event::OpenReplace,
                            )),
                            now,
                        );
                    }
                    menu::Message::OpenFile => Message::Document(document::Message::OpenFile),
                    menu::Message::NewFile => Message::Document(document::Message::NewFile),
                    menu::Message::SaveFile => Message::Document(document::Message::SaveFile),
                    menu::Message::SaveFileAs => Message::Document(document::Message::SaveFileAs),
                    menu::Message::Undo => Message::Document(document::Message::Undo),
                    menu::Message::Redo => Message::Document(document::Message::Redo),
                    menu::Message::OpenProject => Message::OpenProject,
                    menu::Message::NewProject => Message::NewProject,
                    menu::Message::SaveProject => Message::SaveProject,
                    menu::Message::SaveProjectAs => Message::SaveProjectAs,
                    menu::Message::OpenFolder => Message::OpenFolder,
                    menu::Message::CloseFolder => Message::CloseFolder,
                };
                Task::batch([operation::focus(EDITOR), self.react(message, now)])
            }
            Message::Settings(message) => {
                let Some(Modal::Settings(state)) = &mut self.modal else {
                    return Task::none();
                };
                let action = state.update(message, &mut self.settings);
                let task = action.task.map(Message::Settings);

                match action.instruction {
                    Some(settings::Instruction::Changed) => Task::batch([
                        task,
                        self.document
                            .apply(&self.settings, now)
                            .map(Message::Document),
                        self.settings_changed(),
                    ]),
                    Some(settings::Instruction::Previewed) => Task::batch([
                        task,
                        self.document
                            .apply(&self.settings, now)
                            .map(Message::Document),
                    ]),
                    Some(settings::Instruction::Close) => {
                        self.modal = None;
                        Task::batch([task, operation::focus(EDITOR)])
                    }
                    None => task,
                }
            }
            // What is unsaved is asked about once the user picked something to open.
            Message::OpenProject | Message::OpenFolder => {
                if self.refuse() {
                    return Task::none();
                }
                self.notice = None;
                let task = if matches!(message, Message::OpenFolder) {
                    self.opener.open_folder()
                } else {
                    self.opener.open_project()
                };
                task.map(Message::Opener)
            }
            Message::CloseFolder => {
                if !matches!(self.mode, Mode::Folder(_)) {
                    return Task::none();
                }
                self.ask(Pending::CloseFolder, now)
            }
            Message::FolderSettings {
                session,
                handle,
                result,
            } => {
                if session != self.sessions {
                    return Task::none();
                }
                self.is_loading = false;

                let settings = match result {
                    Ok(json) => core::folder::Settings::from_json(&json).map_err(|error| {
                        format!(
                            "{} can't be read ({error}), so bulin keeps it as it is.",
                            core::folder::SETTINGS
                        )
                    }),
                    Err(folder::Error::Missing) => Ok(core::folder::Settings::default()),
                    Err(error) => {
                        self.notice = Some(format!(
                            "Could not read {}: {error}",
                            core::folder::SETTINGS
                        ));
                        Ok(core::folder::Settings::default())
                    }
                };
                let locked = settings.is_err();
                let settings = settings.unwrap_or_else(|notice| {
                    self.notice = Some(notice);
                    core::folder::Settings::default()
                });

                // The analyzer starts before the folder queues its first writes, so their changes
                // reach it. The scan opens the main file and attaches it then.
                let events = self.restart(analyzer::Workspace::Folder(handle.clone()));
                self.document = document::State::new("", &self.settings);
                self.uniforms = uniforms::State::new(settings.uniforms.clone(), settings.clock);
                let (state, task) = folder::State::new(handle, self.sessions, settings, locked);
                self.mode = Mode::Folder(Box::new(state));
                self.program_changed();
                Task::batch([events, task.map(Message::Folder)])
            }
            Message::NewProject => {
                if !matches!(self.mode, Mode::Project { .. }) {
                    return Task::none();
                }
                self.ask(Pending::NewProject, now)
            }
            Message::SaveProject => {
                if self.refuse() {
                    return Task::none();
                }
                let Mode::Project { file, .. } = &self.mode else {
                    return Task::none();
                };
                let file = file.clone();
                self.save_project(file)
            }
            Message::SaveProjectAs => {
                if self.refuse() || !matches!(self.mode, Mode::Project { .. }) {
                    return Task::none();
                }
                self.save_project(None)
            }
            Message::ProjectSaved(result) => {
                self.is_loading = false;

                let saved = match result {
                    Ok((name, shader, uniforms)) => {
                        if let Mode::Project { file, dirty, text } = &mut self.mode {
                            *file = Some(name);
                            *text = shader;
                            // Uniforms edited while the dialog was open aren't saved.
                            if *self.uniforms.uniforms() == uniforms {
                                *dirty = false;
                            }
                        }
                        true
                    }
                    Err(file::Error::DialogClosed) => false,
                    Err(error) => {
                        self.notice = Some(format!("Could not save the project: {error}"));
                        false
                    }
                };

                // Only a Save answered at a prompt waits for the project; a failed or cancelled
                // save leaves what waits undone.
                match self
                    .draining
                    .take_if(|drain| matches!(drain.step, Step::Project))
                {
                    Some(drain) if saved => self.start(drain.then, now),
                    Some(_) | None => Task::none(),
                }
            }
            // A question already open decides first.
            Message::CloseRequested => self.ask(Pending::Quit, now),
            Message::Analyzer(generation, event) => {
                // The client asserts an event is its own, and an event doesn't say whose it is.
                if generation != self.generation {
                    return Task::none();
                }
                let action = self.document.lsp(running(&mut self.analyzer), event);
                match action.instruction {
                    Some(document::Instruction::Changed) => self.program_changed(),
                    Some(document::Instruction::Save { .. })
                    | Some(document::Instruction::FileSaved(_))
                    | None => {}
                }
                action.task.map(Message::Document)
            }
            Message::KeyPressed {
                key,
                physical,
                modifiers,
            } => {
                let command = settings::keymap::chord(&key, physical, modifiers)
                    .and_then(|chord| self.settings.keybindings.command(&chord));
                // The editor already ran its own commands' chords.
                match command {
                    Some(core::settings::Command::ToggleInlayHints) => {
                        self.settings.editor.inlay_hints = !self.settings.editor.inlay_hints;
                    }
                    Some(core::settings::Command::ToggleWhitespace) => {
                        self.settings.editor.render_whitespace =
                            !self.settings.editor.render_whitespace;
                    }
                    _ => return Task::none(),
                }

                Task::batch([
                    self.document
                        .apply(&self.settings, now)
                        .map(Message::Document),
                    self.settings_changed(),
                ])
            }
            Message::SettingsSaved(result) => {
                self.saving_settings = false;
                if let Err(error) = result {
                    self.notice = Some(format!("Could not save the settings: {error}"));
                }

                if self.settings_stale {
                    self.settings_stale = false;
                    self.settings_changed()
                } else {
                    Task::none()
                }
            }
        }
    }

    /// Writes the settings, or has them written again once the write in progress ends.
    fn settings_changed(&mut self) -> Task<Message> {
        if self.saving_settings {
            self.settings_stale = true;
            return Task::none();
        }

        if self.settings_unreadable {
            if let Err(error) = settings::storage::back_up() {
                self.notice = Some(format!("Could not back up the settings: {error}"));
                return Task::none();
            }
            self.settings_unreadable = false;
        }

        self.saving_settings = true;
        Task::perform(
            settings::storage::save(self.settings.clone()),
            Message::SettingsSaved,
        )
    }

    /// Why a file action can't run now, if it can't.
    fn busy(&self) -> Option<Busy> {
        if self.prompt.is_some() {
            Some(Busy::Prompt)
        } else if self.is_loading || self.document.is_loading() || self.opener.is_busy() {
            Some(Busy::Dialog)
        } else if self.draining.is_some() {
            Some(Busy::Draining)
        } else {
            None
        }
    }

    /// Whether a file action must wait, telling the user why unless a dialog or a question shows
    /// it.
    fn refuse(&mut self) -> bool {
        match self.busy() {
            Some(Busy::Prompt | Busy::Dialog) => true,
            Some(Busy::Draining) => {
                self.notice = Some(DRAINING.to_owned());
                true
            }
            None => false,
        }
    }

    /// Whether anything has unsaved changes: the project or its file, or the folder's document.
    fn is_dirty(&self) -> bool {
        match &self.mode {
            Mode::Project { .. } => self.is_project_dirty() || self.is_file_dirty(),
            Mode::Folder(_) => folder::unsaved(&self.document),
        }
    }

    /// Whether the project's shader, uniforms or clock changed since it was last loaded or saved,
    /// whatever file the document is.
    fn is_project_dirty(&self) -> bool {
        match &self.mode {
            Mode::Project { dirty, text, .. } => {
                *dirty || !same_text(text, &self.document.content())
            }
            Mode::Folder(_) => false,
        }
    }

    /// Whether the document is a file picked in a dialog, with unsaved changes.
    fn is_file_dirty(&self) -> bool {
        matches!(self.document.location(), document::Location::File(_)) && self.document.is_dirty()
    }

    /// Does `then` unless the app is busy, asking first when it would replace unsaved changes.
    fn ask(&mut self, then: Pending, now: Instant) -> Task<Message> {
        if self.refuse() {
            return Task::none();
        }
        if self.is_dirty() {
            self.raise(Prompt::Unsaved { then });
            return Task::none();
        }
        self.start(then, now)
    }

    /// Shows `prompt`, and has the folder leave its document alone until the user answers. A
    /// question while saves finish means one of them needs the user, so what waits for them
    /// doesn't run.
    fn raise(&mut self, prompt: Prompt) {
        if let Mode::Folder(state) = &mut self.mode {
            state.hold();
        }
        self.draining = None;
        self.prompt = Some(prompt);
    }

    /// Starts `then`. Leaving a folder first writes its settings and waits for its writes.
    fn start(&mut self, then: Pending, now: Instant) -> Task<Message> {
        let leave = match &mut self.mode {
            Mode::Folder(state) if then.leaves() => state
                .leave(self.uniforms.uniforms(), self.uniforms.clock())
                .map(Message::Folder),
            Mode::Folder(_) | Mode::Project { .. } => return self.run(then, now),
        };
        self.draining = Some(Drain {
            then,
            step: Step::Queue,
        });
        Task::batch([leave, self.drain(now)])
    }

    /// Saves what is unsaved from `step` on, the document's file before the project, then starts
    /// `then`. A failed or cancelled save leaves `then` undone.
    fn save_from(&mut self, then: Pending, step: Step, now: Instant) -> Task<Message> {
        if matches!(step, Step::File) && self.is_file_dirty() {
            self.draining = Some(Drain {
                then,
                step: Step::File,
            });
            // Straight to the document: draining would refuse its save.
            let action = self.document.update(
                document::Message::SaveFile,
                now,
                running(&mut self.analyzer),
                &self.settings,
            );
            return action.task.map(Message::Document);
        }
        if let Mode::Project { file, .. } = &self.mode
            && self.is_project_dirty()
        {
            let file = file.clone();
            self.draining = Some(Drain {
                then,
                step: Step::Project,
            });
            return self.save_project(file);
        }
        self.start(then, now)
    }

    /// Saves the project to `file`, or where the user says.
    fn save_project(&mut self, file: Option<file::Name>) -> Task<Message> {
        self.is_loading = true;
        self.notice = None;
        let shader = self.document.content();
        let uniforms = self.uniforms.uniforms().clone();
        let json =
            core::Project::new(shader.clone(), uniforms.clone(), self.uniforms.clock()).to_json();

        Task::perform(
            async move {
                let name = file::save_project(file, json).await?;
                Ok((name, shader, uniforms))
            },
            Message::ProjectSaved,
        )
    }

    /// Runs what waits for the folder's writes, once they are done.
    fn drain(&mut self, now: Instant) -> Task<Message> {
        if let Mode::Folder(state) = &self.mode
            && !state.is_idle()
        {
            return Task::none();
        }
        match self
            .draining
            .take_if(|drain| matches!(drain.step, Step::Queue))
        {
            Some(drain) => self.run(drain.then, now),
            None => Task::none(),
        }
    }

    /// Does `then`, now that nothing unsaved or unwritten stands in its way.
    fn run(&mut self, then: Pending, now: Instant) -> Task<Message> {
        match then {
            Pending::OpenFolder(handle) => {
                self.sessions += 1;
                self.is_loading = true;
                let session = self.sessions;
                let settings = core::folder::Path::new(core::folder::SETTINGS)
                    .expect("invariant: the settings path is valid");

                Task::perform(folder::read(handle.clone(), settings), move |result| {
                    Message::FolderSettings {
                        session,
                        handle,
                        result,
                    }
                })
            }
            Pending::OpenProject(name, text) => match core::Project::from_json(&text) {
                Ok(project) => {
                    let events = self.leave();
                    self.load(Some(name), project);
                    events
                }
                Err(error) => {
                    self.notice = Some(format!("Could not open the project: {error}"));
                    Task::none()
                }
            },
            Pending::CloseFolder => {
                let events = self.leave();
                self.load(None, core::Project::untitled());
                events
            }
            Pending::OpenDocument(path) => match &mut self.mode {
                Mode::Folder(state) => state
                    .open(
                        path,
                        &mut self.document,
                        running(&mut self.analyzer),
                        &self.settings,
                    )
                    .map(Message::Folder),
                Mode::Project { .. } => Task::none(),
            },
            Pending::NewProject => {
                self.notice = None;
                self.load(None, core::Project::untitled());
                Task::none()
            }
            Pending::OpenFile | Pending::NewFile => {
                let message = if matches!(then, Pending::OpenFile) {
                    document::Message::OpenFile
                } else {
                    document::Message::NewFile
                };
                let action =
                    self.document
                        .update(message, now, running(&mut self.analyzer), &self.settings);
                if matches!(action.instruction, Some(document::Instruction::Changed)) {
                    self.program_changed();
                }
                action.task.map(Message::Document)
            }
            // Nothing queued after this runs.
            Pending::Quit => iced::exit(),
        }
    }

    /// Starts the scratch analyzer when leaving a folder for a project, returning its events.
    fn leave(&mut self) -> Task<Message> {
        match self.mode {
            Mode::Folder(_) => self.restart(analyzer::Workspace::Scratch),
            Mode::Project { .. } => Task::none(),
        }
    }

    /// Replaces the analyzer with one on `workspace`, numbered by the next session. The document
    /// leaves the old one first, since an editor can't open on a new client while open on another.
    /// Returns the new analyzer's events.
    fn restart(&mut self, workspace: analyzer::Workspace) -> Task<Message> {
        self.document.detach(running(&mut self.analyzer));
        self.analyzer = None;

        self.sessions += 1;
        self.generation = self.sessions;
        let (analyzer, events) = analyzer::Analyzer::start(workspace);
        self.analyzer = Some(analyzer);
        let (events, handle) =
            Task::run(events, Message::Analyzer.with(self.generation)).abortable();
        self.analyzer_events = handle.abort_on_drop();
        events
    }

    /// Shows the current program and gives the analyzer its uniforms package.
    fn program_changed(&mut self) {
        let time = self.uniforms.time();
        let uniforms = self.uniforms.uniforms();
        match &mut self.mode {
            Mode::Project { .. } => {
                let program =
                    core::Program::project(self.document.content(), uniforms.clone(), time);
                self.viewer.load(&program);
            }
            // Until the first scan the folder's files are unknown, and compiling would report
            // the main file missing.
            Mode::Folder(state) if !state.is_scanned() => {
                self.viewer.fail("Loading the folder…".to_owned());
            }
            Mode::Folder(state) => match state.program(uniforms.clone(), time) {
                Some(Ok(program)) => {
                    let read = self.viewer.load(&program);
                    state.compiled(read);
                }
                Some(Err(error)) => self.viewer.fail(error.to_string()),
                None => self
                    .viewer
                    .fail("This folder has no main file. Set one in the explorer.".to_owned()),
            },
        }
        running(&mut self.analyzer).set_uniforms(&core::program::uniforms_package(time, uniforms));
    }

    /// Makes `project`, saved in `file` when it is, what the editor works on.
    fn load(&mut self, file: Option<file::Name>, project: core::Project) {
        self.document.detach(running(&mut self.analyzer));
        self.document = document::State::new(&project.shader, &self.settings);
        self.document.attach(running(&mut self.analyzer));
        self.uniforms = uniforms::State::new(project.uniforms, project.clock);
        self.mode = Mode::Project {
            file,
            dirty: false,
            text: project.shader,
        };
        self.program_changed();
    }

    pub fn view(&self) -> Element<'_, Message> {
        let panes = self
            .layout
            .view(|_, pane, _| match pane {
                layout::PaneContent::Editor => {
                    let editors = column![
                        self.uniforms.view().map(Message::Uniforms),
                        self.document.view().map(Message::Document),
                    ];

                    (container(editors).into(), None)
                }
                layout::PaneContent::Viewer => (
                    self.viewer.view().into(),
                    self.viewer
                        .error()
                        .map(|error| format!("Shader error: {error}")),
                ),
            })
            .on_drag(|e| Message::Layout(layout::Message::Dragged(e)))
            .on_resize(10, |e| Message::Layout(layout::Message::Resized(e)));

        let mut document = self.document.label().to_owned();
        if self.is_dirty() {
            document.push_str(" •");
        }
        let (location, status) = match &self.mode {
            Mode::Project { .. } => (document, None),
            Mode::Folder(state) => (format!("{} · {document}", state.name()), state.status()),
        };
        let title = [Some(location.as_str()), self.notice.as_deref()]
            .into_iter()
            .chain([self.document.failure(), status])
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        // Equal-width sides keep the title centered whatever the menu and status measure.
        let header = row![
            container(menu_icon()).width(Fill),
            text(title),
            container(text(self.document.status(self.analyzer.as_ref().expect(RUNNING))).size(14))
                .width(Fill)
                .align_right(Fill),
        ]
        .align_y(Center)
        .padding([0, 10]);

        let panes = container(panes).width(Length::Fill).height(Length::Fill);
        let body: Element<'_, Message> = match &self.mode {
            Mode::Project { .. } => panes.into(),
            Mode::Folder(state) => {
                row![state.view(&self.document).map(Message::Folder), panes].into()
            }
        };
        let content = column![header, body];
        // The library entry that is open, which the library mustn't rename or delete.
        let current = match &self.mode {
            Mode::Folder(state) => state.handle().library_id(),
            Mode::Project { file, .. } => file.as_ref().and_then(file::Name::library_id),
        };

        // The content keeps its place in the widget tree whether a modal is open or not, so
        // opening one doesn't reset the editor's document, the combo box or the panes.
        let layer = self.modal.as_ref().map(|layer| match layer {
            Modal::Menu => menu::view(&self.mode, self.settings.editor.find).map(Message::Menu),
            Modal::Settings(state) => state.view(&self.settings).map(Message::Settings),
        });

        stack![content]
            .push(layer.map(|layer| modal(layer, Some(Message::CloseModal))))
            .push(
                self.opener
                    .view(current)
                    .map(|opener| modal(opener.map(Message::Opener), Some(Message::Escape))),
            )
            // Only an answer closes a question.
            .push(
                self.prompt
                    .as_ref()
                    .map(|prompt| modal(prompt.view().map(Message::Prompt), None)),
            )
            .into()
    }

    pub fn theme(&self) -> Theme {
        self.document.theme()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        // The chord being recorded must reach no shortcut, no editor and not the Escape that
        // closes the modal. The clock takes no keys, so the preview keeps running.
        if let Some(Modal::Settings(state)) = &self.modal
            && state.is_recording()
        {
            return Subscription::batch([
                state.subscription().map(Message::Settings),
                self.uniforms.subscription().map(Message::Uniforms),
            ]);
        }

        // Escape answers a question or closes the library or a modal even while the editor holds
        // focus and captures keys.
        let escape = event::listen_with(|event, _status, _window| match event {
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(key::Named::Escape),
                ..
            }) => Some(Message::Escape),
            _ => None,
        });

        let shortcuts = keyboard::listen().filter_map(|event| match event {
            keyboard::Event::KeyPressed {
                key,
                physical_key,
                modifiers,
                repeat: false,
                ..
            } => Some(Message::KeyPressed {
                key,
                physical: physical_key,
                modifiers,
            }),
            _ => None,
        });
        let folder = match &self.mode {
            Mode::Folder(state) => state.subscription().map(Message::Folder),
            Mode::Project { .. } => Subscription::none(),
        };

        Subscription::batch([
            self.document.subscription().map(Message::Document),
            self.uniforms.subscription().map(Message::Uniforms),
            folder,
            escape,
            shortcuts,
            window::close_requests().map(|_| Message::CloseRequested),
        ])
    }
}

/// Why an open, close or file action is refused while a folder's writes finish.
const DRAINING: &str = "Wait until the folder's files are saved.";

/// Why the analyzer is always there to use.
const RUNNING: &str = "invariant: an analyzer runs except inside a restart";

/// The running analyzer.
fn running(analyzer: &mut Option<analyzer::Analyzer>) -> &mut analyzer::Analyzer {
    analyzer.as_mut().expect(RUNNING)
}

fn menu_icon<'a>() -> Element<'a, Message> {
    const MENU_FONT: Font = Font::new("menu");

    button(text('\u{0e9bd}').font(MENU_FONT))
        .on_press(Message::ShowMenu)
        .into()
}

/// `content` over a dimmed backdrop that swallows input, sending `on_blur` when clicked if there
/// is one.
fn modal<'a>(
    content: impl Into<Element<'a, Message>>,
    on_blur: Option<Message>,
) -> Element<'a, Message> {
    let backdrop = center(opaque(content)).style(|_theme| container::Style {
        background: Some(color!(0x000000, 0.8).into()),
        ..container::Style::default()
    });
    match on_blur {
        Some(on_blur) => opaque(mouse_area(backdrop).on_press(on_blur)),
        None => opaque(backdrop),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;
    use std::{fs, thread};

    use iced::Task;
    use iced::advanced::widget::{Tree, tree};
    use iced::futures::{FutureExt, StreamExt};
    use iced::keyboard::{Key, Modifiers, key};
    use iced::time::Instant;

    use tempfile::TempDir;

    use super::{
        Application, DRAINING, Drain, Message, Modal, Mode, Pending, Step, analyzer, document,
        explorer, folder, uniforms,
    };
    use crate::core;
    use crate::core::folder::{Change, Entry, Expect, Kind, Path, SETTINGS, Settings};
    use crate::core::uniform::Type;
    use crate::core::{Project, Uniforms};
    use crate::file;
    use crate::menu;
    use crate::prompt::{Answer, Prompt};
    use crate::settings;

    const MAIN: &str = "import package::lib::shade;\n\
        fn mainBulin(pixel: bulin::Pixel) -> vec4<f32> {\n\
            return shade(pixel.uv);\n\
        }\n";
    const LIB: &str = "fn shade(uv: vec2<f32>) -> vec4<f32> {\n\
            return vec4<f32>(uv, 0.0, 1.0);\n\
        }\n";
    const OTHER: &str = "fn unused() -> f32 {\n    return 1.0;\n}\n";

    fn path(text: &str) -> Path {
        Path::new(text).unwrap()
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
    }

    /// A folder whose main file imports `lib.wesl`, next to `other.wesl`.
    fn folder() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings::new(Some(path("main.wesl")), Uniforms::default(), None);
        fs::create_dir(dir.path().join(".bulin")).unwrap();
        for (file, text) in [
            ("main.wesl", MAIN),
            ("lib.wesl", LIB),
            ("other.wesl", OTHER),
            (SETTINGS, &settings.to_json()),
        ] {
            fs::write(dir.path().join(file), text).unwrap();
        }
        dir
    }

    /// Drives the app the way the runtime would, running tasks through the storage functions.
    struct Driver {
        app: Application,
        runtime: tokio::runtime::Runtime,
        /// The task the last update returned, which the driver doesn't run.
        task: Task<Message>,
    }

    impl Driver {
        fn new() -> Self {
            Self {
                app: Application::new().0,
                runtime: runtime(),
                task: Task::none(),
            }
        }

        fn update(&mut self, message: Message) {
            self.task = self.app.update(message, Instant::now());
        }

        /// Picks the folder at `dir` and, once the app reads its settings, hands them over.
        fn open(&mut self, dir: &TempDir) {
            let root = dir.path().to_owned();
            self.update(Message::Opener(folder::opener::Message::FolderPicked(
                Some(root.clone()),
            )));
            // Only a pick that isn't refused or waiting for the queue reads the settings.
            if self.app.draining.is_some() || !self.app.is_loading {
                return;
            }
            self.settings(&root);
        }

        /// Reads the settings of the folder at `root`, as opening it does.
        fn settings(&mut self, root: &std::path::Path) {
            let handle = folder::Handle::new(root.to_owned());
            let result = self
                .runtime
                .block_on(folder::read(handle.clone(), path(SETTINGS)));
            self.update(Message::FolderSettings {
                session: self.app.sessions,
                handle,
                result,
            });
        }

        /// Runs the folder's ops until none is left, returning the path each one wrote, created,
        /// moved or removed.
        fn pump(&mut self) -> Vec<String> {
            let mut done = Vec::new();
            loop {
                let Mode::Folder(state) = &self.app.mode else {
                    return done;
                };
                let Some(op) = state.in_flight().cloned() else {
                    return done;
                };
                let session = self.app.sessions;
                let message = match op {
                    folder::Op::Write(write) => {
                        done.push(write.path.as_str().to_owned());
                        let result = self.runtime.block_on(folder::write(
                            write.handle,
                            write.path.clone(),
                            write.text.clone(),
                            write.expect.clone(),
                        ));
                        folder::Message::Wrote {
                            session,
                            path: write.path,
                            text: write.text,
                            expect: write.expect,
                            result,
                        }
                    }
                    folder::Op::CreateDir { handle, path } => {
                        done.push(path.as_str().to_owned());
                        let result = self
                            .runtime
                            .block_on(folder::create_dir(handle, path.clone()));
                        folder::Message::DirCreated {
                            session,
                            path,
                            result,
                        }
                    }
                    folder::Op::Rename { handle, from, to } => {
                        done.push(from.as_str().to_owned());
                        let result =
                            self.runtime
                                .block_on(folder::rename(handle, from.clone(), to.clone()));
                        folder::Message::Renamed {
                            session,
                            from,
                            to,
                            result,
                        }
                    }
                    folder::Op::Remove { handle, path } => {
                        done.push(path.as_str().to_owned());
                        let result = self.runtime.block_on(folder::remove(handle, path.clone()));
                        folder::Message::Removed {
                            session,
                            path,
                            result,
                        }
                    }
                };
                self.update(Message::Folder(message));
            }
        }

        /// Reports the files of `dir` that exist, as the watcher's scan does.
        fn scan(&mut self, dir: &TempDir, session: u64) {
            let scan = ["main.wesl", "lib.wesl", "other.wesl", "wesl.toml"]
                .into_iter()
                .filter_map(|file| {
                    let text = fs::read_to_string(dir.path().join(file)).ok()?;
                    Some((path(file), Some(text)))
                })
                .collect();
            self.update(Message::Folder(folder::Message::Watched(
                session,
                folder::Watched::Scan(scan),
            )));
        }

        fn changes(&mut self, changes: Vec<Change>) {
            self.update(Message::Folder(folder::Message::Watched(
                self.app.sessions,
                folder::Watched::Changes(changes),
            )));
        }

        /// Opens `dir` and runs its first writes and scan.
        fn enter(&mut self, dir: &TempDir) {
            self.open(dir);
            self.pump();
            self.scan(dir, self.app.sessions);
        }

        fn explore(&mut self, message: explorer::Message) {
            self.update(Message::Folder(folder::Message::Explorer(message)));
        }

        fn type_text(&mut self, text: &str) {
            for character in text.chars() {
                self.update(Message::Document(document::Message::Editor(
                    matcha_iced::Event::Editor(matcha_iced::Action::Type(character)),
                )));
            }
        }

        fn save(&mut self) {
            self.update(Message::Document(document::Message::SaveFile));
        }

        fn is_folder(&self) -> bool {
            matches!(self.app.mode, Mode::Folder(_))
        }

        fn answer(&mut self, answer: Answer) {
            self.update(Message::Prompt(answer));
        }

        /// Runs the last update's task to its end, returning the messages it produced.
        fn outputs(&mut self) -> Vec<Message> {
            let task = std::mem::replace(&mut self.task, Task::none());
            let Some(stream) = iced_runtime::task::into_stream(task) else {
                return Vec::new();
            };
            self.runtime.block_on(
                stream
                    .filter_map(|action| async move {
                        match action {
                            iced_runtime::Action::Output(message) => Some(message),
                            _ => None,
                        }
                    })
                    .collect(),
            )
        }

        /// Whether the last update's task exits the app.
        fn exits(&mut self) -> bool {
            let task = std::mem::replace(&mut self.task, Task::none());
            let Some(mut stream) = iced_runtime::task::into_stream(task) else {
                return false;
            };
            while let Some(Some(action)) = stream.next().now_or_never() {
                if matches!(action, iced_runtime::Action::Exit) {
                    return true;
                }
            }
            false
        }
    }

    /// Presses Ctrl+Alt with the letter `c`, as no widget took it.
    fn press_ctrl_alt(app: &mut Application, c: &str, code: key::Code) {
        let _ = app.update(
            Message::KeyPressed {
                key: Key::Character(c.into()),
                physical: key::Physical::Code(code),
                modifiers: Modifiers::CTRL | Modifiers::ALT,
            },
            Instant::now(),
        );
    }

    fn changed(file: &str, text: &str) -> Change {
        Change::Changed {
            path: path(file),
            text: Some(text.to_owned()),
        }
    }

    /// The address of a node's widget state, which stays the same only while iced keeps it.
    fn state_address(tree: &Tree) -> Option<*const ()> {
        match &tree.state {
            tree::State::Some(state) => Some(&**state as *const _ as *const ()),
            tree::State::None => None,
        }
    }

    /// Whether showing `modal` over the content keeps the content's widget state.
    fn keeps_the_content_state(modal: Modal) -> bool {
        let (mut app, _) = Application::with_settings(core::Settings::default());
        let mut tree = Tree::empty();
        tree.diff(&mut app.view());
        let root = tree.tag;
        let content = state_address(&tree.children[0]);

        app.modal = Some(modal);
        tree.diff(&mut app.view());

        // A root of another kind would make iced rebuild the whole tree and drop its state.
        tree.tag == root && content.is_some() && state_address(&tree.children[0]) == content
    }

    #[test]
    fn opening_the_menu_keeps_the_content_state() {
        assert!(keeps_the_content_state(Modal::Menu));
    }

    #[test]
    fn opening_the_settings_keeps_the_content_state() {
        assert!(keeps_the_content_state(Modal::Settings(
            settings::State::new()
        )));
    }

    #[test]
    fn a_shortcut_toggle_survives_a_change_in_the_open_settings() {
        let (mut app, _) = Application::with_settings(core::Settings::default());
        let _ = app.update(Message::Menu(menu::Message::Settings), Instant::now());
        let hints = app.settings.editor.inlay_hints;
        let whitespace = app.settings.editor.render_whitespace;

        press_ctrl_alt(&mut app, "i", key::Code::KeyI);
        let _ = app.update(
            Message::Settings(settings::Message::ToggleWhitespace(!whitespace)),
            Instant::now(),
        );

        assert!(matches!(app.modal, Some(Modal::Settings(_))));
        assert_eq!(app.settings.editor.inlay_hints, !hints);
        assert_eq!(app.settings.editor.render_whitespace, !whitespace);
    }

    #[test]
    fn the_inlay_hints_shortcut_toggles_and_saves_them() {
        let (mut app, _) = Application::with_settings(core::Settings::default());
        let shown = app.settings.editor.inlay_hints;

        press_ctrl_alt(&mut app, "i", key::Code::KeyI);

        assert_eq!(app.settings.editor.inlay_hints, !shown);
        assert!(app.saving_settings);
    }

    #[test]
    fn a_rebound_toggle_answers_its_new_chord_only() {
        let mut settings = core::Settings::default();
        settings.keybindings.set(
            core::settings::Command::ToggleWhitespace,
            "ctrl+alt+e".parse().unwrap(),
        );
        let (mut app, _) = Application::with_settings(settings);
        let shown = app.settings.editor.render_whitespace;

        press_ctrl_alt(&mut app, "w", key::Code::KeyW);
        assert_eq!(app.settings.editor.render_whitespace, shown);

        press_ctrl_alt(&mut app, "e", key::Code::KeyE);
        assert_eq!(app.settings.editor.render_whitespace, !shown);
    }

    #[test]
    fn a_chord_recorded_in_the_settings_is_bound_and_saved() {
        let (mut app, _) = Application::with_settings(core::Settings::default());
        let command = core::settings::Command::ToggleWhitespace;
        let _ = app.update(Message::Menu(menu::Message::Settings), Instant::now());
        let _ = app.update(
            Message::Settings(settings::Message::Record(command)),
            Instant::now(),
        );
        assert!(matches!(&app.modal, Some(Modal::Settings(state)) if state.is_recording()));

        let _ = app.update(
            Message::Settings(settings::Message::Recorded {
                key: Key::Character("e".into()),
                physical: key::Physical::Code(key::Code::KeyE),
                modifiers: Modifiers::CTRL,
            }),
            Instant::now(),
        );

        assert!(matches!(&app.modal, Some(Modal::Settings(state)) if !state.is_recording()));
        assert_eq!(
            app.settings.keybindings.chords(command),
            ["ctrl+e".parse().unwrap()]
        );
        assert!(app.saving_settings);
    }

    #[test]
    fn cancelling_a_recording_keeps_the_binding_and_the_settings_open() {
        let (mut app, _) = Application::with_settings(core::Settings::default());
        let _ = app.update(Message::Menu(menu::Message::Settings), Instant::now());
        let _ = app.update(
            Message::Settings(settings::Message::Record(
                core::settings::Command::ToggleWhitespace,
            )),
            Instant::now(),
        );

        let _ = app.update(
            Message::Settings(settings::Message::CancelRecording),
            Instant::now(),
        );

        assert!(matches!(&app.modal, Some(Modal::Settings(state)) if !state.is_recording()));
        assert_eq!(
            app.settings.keybindings,
            core::Settings::default().keybindings
        );
        assert!(!app.saving_settings);
    }

    #[test]
    fn a_change_during_a_save_is_saved_after_it() {
        let (mut app, _) = Application::with_settings(core::Settings::default());

        press_ctrl_alt(&mut app, "i", key::Code::KeyI);
        press_ctrl_alt(&mut app, "w", key::Code::KeyW);
        assert!(app.saving_settings);
        assert!(app.settings_stale);

        let _ = app.update(Message::SettingsSaved(Ok(())), Instant::now());
        assert!(app.saving_settings);
        assert!(!app.settings_stale);
    }

    #[test]
    fn open_folder_set_main_save_recompiles_only_for_relevant_modules() {
        let dir = folder();
        let mut driver = Driver::new();

        driver.enter(&dir);
        assert!(driver.is_folder());
        assert_eq!(driver.app.document.content(), MAIN);
        assert_eq!(driver.app.viewer.error(), None);

        driver.type_text("@");
        assert_eq!(driver.app.viewer.error(), None, "typing doesn't recompile");
        driver.save();
        driver.pump();
        let error = driver
            .app
            .viewer
            .error()
            .expect("saving recompiles")
            .to_owned();
        assert!(error.starts_with("main.wesl"), "{error}");
        assert!(!driver.app.document.is_dirty());

        let broken = format!("@{MAIN}");
        driver.changes(vec![changed("main.wesl", &broken)]);
        assert!(!driver.app.document.is_dirty());
        assert_eq!(driver.app.notice, None);

        driver.changes(vec![changed("main.wesl", MAIN)]);
        assert_eq!(
            driver.app.document.content(),
            MAIN,
            "a clean document reloads"
        );
        assert_eq!(driver.app.viewer.error(), None);

        driver.app.viewer.fail("unchanged".to_owned());
        driver.changes(vec![changed("other.wesl", "broken(")]);
        assert_eq!(driver.app.viewer.error(), Some("unchanged"));
        driver.changes(vec![changed("lib.wesl", &LIB.replace("0.0", "0.5"))]);
        assert_eq!(driver.app.viewer.error(), None);
    }

    #[test]
    fn nothing_compiles_before_the_first_scan() {
        let dir = folder();
        let mut driver = Driver::new();

        driver.open(&dir);
        driver.pump();
        assert_eq!(driver.app.viewer.error(), Some("Loading the folder…"));

        driver.scan(&dir, driver.app.sessions);
        assert_eq!(driver.app.viewer.error(), None);
    }

    #[test]
    fn write_queue_order_and_stale_results() {
        let (a, b) = (folder(), folder());
        let mut driver = Driver::new();
        driver.open(&a);

        assert_eq!(
            driver.pump(),
            [
                ".bulin/bulin/wesl.toml",
                ".bulin/bulin/package.wesl",
                ".bulin/uniforms/wesl.toml",
                ".bulin/uniforms/package.wesl",
                SETTINGS,
                "wesl.toml",
            ]
        );
        driver.scan(&a, 2);

        driver.open(&b);
        assert_eq!(driver.pump(), [SETTINGS], "leaving a writes its settings");
        driver.settings(b.path());
        assert_eq!(driver.app.sessions, 4);
        driver.update(Message::Folder(folder::Message::Wrote {
            session: 2,
            path: path("main.wesl"),
            text: "late".to_owned(),
            expect: Expect::Anything,
            result: Ok(vec![changed("main.wesl", "late")]),
        }));
        let Mode::Folder(state) = &driver.app.mode else {
            panic!("b is open");
        };
        assert!(state.in_flight().is_some(), "b's writes are untouched");
        driver.scan(&a, 2);
        assert_eq!(driver.app.document.content(), "", "a's scan is dropped");

        driver.pump();
        driver.scan(&b, 4);
        assert_eq!(driver.app.document.content(), MAIN);
    }

    #[test]
    fn close_folder_and_open_project_wait_for_the_queue() {
        let dir = folder();
        let mut driver = Driver::new();
        driver.open(&dir);

        driver.update(Message::CloseFolder);
        assert!(driver.app.draining.is_some());
        assert!(driver.is_folder());
        driver.update(Message::OpenFolder);
        assert_eq!(driver.app.notice.as_deref(), Some(DRAINING));
        assert!(!driver.app.opener.is_busy());
        driver.app.notice = None;
        driver.update(Message::OpenProject);
        assert_eq!(driver.app.notice.as_deref(), Some(DRAINING));

        driver.pump();
        assert!(!driver.is_folder());
        assert!(driver.app.draining.is_none());
        assert_eq!(driver.app.document.content(), Project::untitled().shader);

        driver.enter(&dir);
        let json = include_str!("../assets/test_project.bulin");
        let removed = vec![Change::Removed(path(".bulin/bulin/package.wesl"))];
        driver.changes(removed);
        driver.update(Message::Opener(folder::opener::Message::ProjectPicked(Ok(
            (
                file::Name::from("/projects/test.bulin"),
                Arc::new(json.to_owned()),
            ),
        ))));
        assert!(
            driver.is_folder(),
            "the regenerated files are still being written"
        );
        driver.pump();
        assert!(matches!(
            driver.app.mode,
            Mode::Project { file: Some(_), .. }
        ));
        assert_eq!(
            driver.app.document.content(),
            Project::from_json(json).unwrap().shader
        );
    }

    #[test]
    fn open_folder_a_twice_rescans_and_opens_main() {
        let dir = folder();
        let mut driver = Driver::new();
        driver.enter(&dir);

        driver.open(&dir);
        driver.pump();
        driver.settings(dir.path());
        assert_eq!(driver.app.sessions, 4);
        assert_eq!(driver.app.document.content(), "");

        driver.scan(&dir, 2);
        assert_eq!(
            driver.app.document.content(),
            "",
            "the first opening's scan is dropped"
        );
        driver.pump();
        driver.scan(&dir, 4);
        assert_eq!(driver.app.document.content(), MAIN);
        assert_eq!(driver.app.viewer.error(), None);
    }

    #[test]
    fn a_rescan_after_deletions_removes_them_from_the_mirror() {
        let dir = folder();
        let mut driver = Driver::new();
        driver.enter(&dir);
        assert_eq!(driver.app.viewer.error(), None);

        fs::remove_file(dir.path().join("lib.wesl")).unwrap();
        driver.scan(&dir, driver.app.sessions);

        let error = driver
            .app
            .viewer
            .error()
            .expect("main can't import lib any more");
        assert!(error.contains("lib"), "{error}");
    }

    #[test]
    fn bulin_dir_removed_while_open_is_regenerated_and_saves_succeed() {
        let dir = folder();
        let mut driver = Driver::new();
        driver.enter(&dir);

        fs::remove_dir_all(dir.path().join(".bulin")).unwrap();
        let removed = crate::core::folder::generated("")
            .into_iter()
            .map(|(path, _)| Change::Removed(path))
            .collect();
        driver.changes(removed);
        driver.pump();
        for (file, _) in crate::core::folder::generated("") {
            assert!(dir.path().join(file.as_str()).exists(), "{file} is back");
        }
        assert!(dir.path().join(SETTINGS).exists());

        driver.type_text("// saved\n");
        driver.save();
        driver.pump();
        assert_eq!(
            fs::read_to_string(dir.path().join("main.wesl")).unwrap(),
            format!("// saved\n{MAIN}")
        );
        assert!(!driver.app.document.is_dirty());
    }

    /// What the folder's write in flight writes, with what it expects.
    fn in_flight(driver: &Driver) -> Option<folder::Write> {
        let Mode::Folder(state) = &driver.app.mode else {
            return None;
        };
        match state.in_flight()? {
            folder::Op::Write(write) => Some(write.clone()),
            folder::Op::CreateDir { .. }
            | folder::Op::Rename { .. }
            | folder::Op::Remove { .. } => None,
        }
    }

    /// A project saved in a file of `dir`, whose shader has unsaved changes.
    fn dirty_project(driver: &mut Driver, dir: &TempDir) -> std::path::PathBuf {
        let file = dir.path().join("project.bulin");
        driver.app.mode = Mode::Project {
            file: Some(file::Name::from(file.to_str().unwrap())),
            dirty: false,
            text: driver.app.document.content(),
        };
        driver.type_text("// edited\n");
        file
    }

    #[test]
    fn project_text_edit_asks_and_save_writes_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let mut driver = Driver::new();
        let file = dirty_project(&mut driver, &dir);
        let edited = driver.app.document.content();
        assert!(driver.app.is_dirty());

        driver.update(Message::NewProject);
        assert!(matches!(
            driver.app.prompt,
            Some(Prompt::Unsaved {
                then: Pending::NewProject
            })
        ));
        assert_eq!(driver.app.document.content(), edited);

        driver.answer(Answer::Save);
        assert!(matches!(
            driver.app.draining,
            Some(Drain {
                step: Step::Project,
                ..
            })
        ));
        for message in driver.outputs() {
            driver.update(message);
        }

        let saved = Project::from_json(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(saved.shader, edited);
        assert_eq!(driver.app.document.content(), Project::untitled().shader);
        assert!(matches!(driver.app.mode, Mode::Project { file: None, .. }));
        assert!(driver.app.prompt.is_none());
        assert!(driver.app.draining.is_none());
        assert!(!driver.app.is_dirty());
    }

    /// Opens `text` as the file `dir/shader.wesl`, makes the project hold what the editor does, and
    /// then types, so only the file has unsaved changes.
    fn dirty_file(driver: &mut Driver, dir: &TempDir) -> std::path::PathBuf {
        let file = dir.path().join("shader.wesl");
        fs::write(&file, LIB).unwrap();
        driver.update(Message::Document(document::Message::FileOpened(Ok((
            file::Name::from(file.to_str().unwrap()),
            Arc::new(LIB.to_owned()),
        )))));
        driver.type_text("// edited\n");
        if let Mode::Project { text, .. } = &mut driver.app.mode {
            *text = driver.app.document.content();
        }
        assert!(driver.app.is_file_dirty());
        assert!(!driver.app.is_project_dirty());
        file
    }

    #[test]
    fn quit_with_a_dirty_file_and_a_clean_project_saves_the_file_and_exits() {
        let dir = tempfile::tempdir().unwrap();
        let mut driver = Driver::new();
        let file = dirty_file(&mut driver, &dir);

        driver.update(Message::CloseRequested);
        assert!(matches!(
            driver.app.prompt,
            Some(Prompt::Unsaved {
                then: Pending::Quit
            })
        ));
        assert!(!driver.exits());

        driver.answer(Answer::Save);
        let outputs = driver.outputs();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            format!("// edited\n{LIB}")
        );
        for message in outputs {
            driver.update(message);
        }

        assert!(driver.exits());
        assert!(driver.app.draining.is_none());
        assert!(driver.app.prompt.is_none());
    }

    #[test]
    fn a_failed_save_cancels_and_clears_draining() {
        let dir = tempfile::tempdir().unwrap();
        let mut driver = Driver::new();
        let _ = dirty_file(&mut driver, &dir);

        driver.update(Message::CloseRequested);
        driver.answer(Answer::Save);
        assert!(driver.app.draining.is_some());
        let error = file::Error::Io(Arc::new(std::io::Error::other("disk on fire")));
        driver.update(Message::Document(document::Message::FileSaved(Err(error))));

        assert!(!driver.exits());
        assert!(driver.app.draining.is_none());
        assert!(driver.app.prompt.is_none());
        assert!(driver.app.is_dirty());
    }

    #[test]
    fn quit_in_a_clean_folder_writes_settings_and_waits() {
        let dir = folder();
        let mut driver = Driver::new();
        driver.enter(&dir);

        driver.update(Message::CloseRequested);
        assert!(driver.app.prompt.is_none());
        assert!(!driver.exits());
        assert!(matches!(
            driver.app.draining,
            Some(Drain {
                then: Pending::Quit,
                step: Step::Queue
            })
        ));
        assert_eq!(in_flight(&driver).unwrap().path, path(SETTINGS));

        assert_eq!(driver.pump(), [SETTINGS]);
        assert!(driver.exits());
    }

    /// A way to ask for something that waits for an answer, and whether it has happened.
    struct Case {
        name: &'static str,
        trigger: fn(&mut Driver, &TempDir),
        happened: fn(&mut Driver) -> bool,
    }

    fn pick_folder(driver: &mut Driver, next: &TempDir) {
        driver.update(Message::Opener(folder::opener::Message::FolderPicked(
            Some(next.path().to_owned()),
        )));
    }

    fn pick_project(driver: &mut Driver, _: &TempDir) {
        let json = include_str!("../assets/test_project.bulin");
        driver.update(Message::Opener(folder::opener::Message::ProjectPicked(Ok(
            (
                file::Name::from("/projects/test.bulin"),
                Arc::new(json.to_owned()),
            ),
        ))));
    }

    fn opened_project(driver: &mut Driver) -> bool {
        let json = include_str!("../assets/test_project.bulin");
        matches!(driver.app.mode, Mode::Project { file: Some(_), .. })
            && driver.app.document.content() == Project::from_json(json).unwrap().shader
    }

    /// Whether the app is reading the settings of a folder it opens.
    fn opening_folder(driver: &mut Driver) -> bool {
        driver.app.is_loading
    }

    #[test]
    fn each_prompt_with_each_continuation_in_a_project() {
        let cases = [
            Case {
                name: "new project",
                trigger: |driver, _| driver.update(Message::NewProject),
                happened: |driver| {
                    matches!(driver.app.mode, Mode::Project { file: None, .. })
                        && driver.app.document.content() == Project::untitled().shader
                },
            },
            Case {
                name: "open file",
                trigger: |driver, _| driver.update(Message::Document(document::Message::OpenFile)),
                happened: |driver| driver.app.document.is_loading(),
            },
            Case {
                name: "new file",
                trigger: |driver, _| driver.update(Message::Document(document::Message::NewFile)),
                happened: |driver| {
                    matches!(driver.app.mode, Mode::Project { file: Some(_), .. })
                        && driver.app.document.content() == crate::core::program::DEFAULT_SOURCE
                },
            },
            Case {
                name: "quit",
                trigger: |driver, _| driver.update(Message::CloseRequested),
                happened: Driver::exits,
            },
            Case {
                name: "open folder",
                trigger: pick_folder,
                happened: opening_folder,
            },
            Case {
                name: "open project",
                trigger: pick_project,
                happened: opened_project,
            },
        ];

        for case in cases {
            for answer in [Answer::Save, Answer::Discard, Answer::Cancel] {
                let (dir, next) = (tempfile::tempdir().unwrap(), folder());
                let mut driver = Driver::new();
                let file = dirty_project(&mut driver, &dir);
                let edited = driver.app.document.content();
                let name = format!("{} after {answer:?}", case.name);

                (case.trigger)(&mut driver, &next);
                assert!(
                    matches!(driver.app.prompt, Some(Prompt::Unsaved { .. })),
                    "{name}"
                );
                assert!(!(case.happened)(&mut driver), "{name} waits for the answer");

                driver.answer(answer);
                if answer == Answer::Save {
                    for message in driver.outputs() {
                        driver.update(message);
                    }
                    let saved = Project::from_json(&fs::read_to_string(&file).unwrap()).unwrap();
                    assert_eq!(saved.shader, edited, "{name}");
                }
                assert!(driver.app.prompt.is_none(), "{name}");
                assert!(driver.app.draining.is_none(), "{name}");
                if answer == Answer::Cancel {
                    assert!(!(case.happened)(&mut driver), "{name}");
                    assert_eq!(driver.app.document.content(), edited, "{name}");
                } else {
                    assert!((case.happened)(&mut driver), "{name}");
                }
            }
        }
    }

    #[test]
    fn each_prompt_with_each_continuation_in_a_folder() {
        let cases = [
            Case {
                name: "close folder",
                trigger: |driver, _| driver.update(Message::CloseFolder),
                happened: |driver| !driver.is_folder(),
            },
            Case {
                name: "open document",
                trigger: |driver, _| driver.explore(explorer::Message::Open(path("other.wesl"))),
                happened: |driver| driver.app.document.content() == OTHER,
            },
            Case {
                name: "open folder",
                trigger: pick_folder,
                happened: opening_folder,
            },
            Case {
                name: "open project",
                trigger: pick_project,
                happened: opened_project,
            },
            Case {
                name: "quit",
                trigger: |driver, _| driver.update(Message::CloseRequested),
                happened: Driver::exits,
            },
        ];

        for case in cases {
            for answer in [Answer::Save, Answer::Discard, Answer::Cancel] {
                let (dir, next) = (folder(), folder());
                let mut driver = Driver::new();
                driver.enter(&dir);
                driver.type_text("// edited\n");
                let name = format!("{} after {answer:?}", case.name);

                (case.trigger)(&mut driver, &next);
                assert!(
                    matches!(driver.app.prompt, Some(Prompt::Unsaved { .. })),
                    "{name}"
                );
                assert!(!(case.happened)(&mut driver), "{name} waits for the answer");

                driver.answer(answer);
                driver.pump();
                assert!(driver.app.prompt.is_none(), "{name}");
                assert!(driver.app.draining.is_none(), "{name}");
                let disk = fs::read_to_string(dir.path().join("main.wesl")).unwrap();
                match answer {
                    Answer::Save => {
                        assert_eq!(disk, format!("// edited\n{MAIN}"), "{name}");
                        assert!((case.happened)(&mut driver), "{name}");
                    }
                    Answer::Discard => {
                        assert_eq!(disk, MAIN, "{name}");
                        assert!((case.happened)(&mut driver), "{name}");
                    }
                    Answer::Cancel => {
                        assert_eq!(disk, MAIN, "{name}");
                        assert!(!(case.happened)(&mut driver), "{name}");
                        assert!(driver.app.document.is_dirty(), "{name}");
                    }
                    Answer::Reload | Answer::KeepMine | Answer::Overwrite | Answer::Delete => {
                        unreachable!()
                    }
                }
            }
        }
    }

    #[test]
    fn a_disk_change_during_a_prompt_is_rechecked_on_close() {
        let dir = folder();
        let mut driver = Driver::new();
        driver.enter(&dir);
        driver.type_text("// edited\n");
        let edited = driver.app.document.content();

        driver.update(Message::CloseFolder);
        driver.changes(vec![changed("main.wesl", "// theirs\n")]);
        assert_eq!(driver.app.document.content(), edited);
        assert_eq!(driver.app.document.base(), Some(MAIN));
        assert!(matches!(driver.app.prompt, Some(Prompt::Unsaved { .. })));

        driver.answer(Answer::Cancel);
        assert!(matches!(
            driver.app.prompt,
            Some(Prompt::ChangedOnDisk { deleted: false, .. })
        ));
        driver.changes(vec![changed("main.wesl", "// newest\n")]);
        assert_eq!(driver.app.document.content(), edited);

        driver.answer(Answer::Reload);
        assert_eq!(driver.app.document.content(), "// newest\n");
        assert!(!driver.app.document.is_dirty());
        assert!(driver.app.prompt.is_none());
        assert!(driver.is_folder());
    }

    #[test]
    fn keep_mine_makes_the_next_save_overwrite() {
        let dir = folder();
        let mut driver = Driver::new();
        driver.enter(&dir);
        driver.type_text("// edited\n");
        let edited = driver.app.document.content();

        fs::write(dir.path().join("main.wesl"), "// theirs\n").unwrap();
        driver.changes(vec![changed("main.wesl", "// theirs\n")]);
        driver.answer(Answer::KeepMine);
        assert!(driver.app.prompt.is_none());
        assert_eq!(driver.app.document.content(), edited);

        driver.save();
        driver.pump();
        assert_eq!(
            fs::read_to_string(dir.path().join("main.wesl")).unwrap(),
            edited
        );
        assert!(driver.app.prompt.is_none());
        assert!(!driver.app.document.is_dirty());
    }

    #[test]
    fn a_conflict_during_another_prompt_raises_nothing() {
        let dir = folder();
        let mut driver = Driver::new();
        driver.enter(&dir);
        driver.type_text("// edited\n");
        driver.save();
        let write = in_flight(&driver).unwrap();

        driver.update(Message::CloseFolder);
        driver.update(Message::Folder(folder::Message::Wrote {
            session: driver.app.sessions,
            path: write.path,
            text: write.text,
            expect: write.expect,
            result: Err(folder::Error::Conflict {
                disk: Some("// theirs\n".to_owned()),
            }),
        }));

        assert!(matches!(
            driver.app.prompt,
            Some(Prompt::Unsaved {
                then: Pending::CloseFolder
            })
        ));
        assert!(driver.app.document.is_dirty());
        assert_eq!(driver.app.document.base(), Some(MAIN));

        driver.answer(Answer::Cancel);
        assert!(matches!(
            driver.app.prompt,
            Some(Prompt::ChangedOnDisk { deleted: false, .. })
        ));
    }

    #[test]
    fn overwrite_writes_the_current_editor_text() {
        let dir = folder();
        let mut driver = Driver::new();
        driver.enter(&dir);
        fs::write(dir.path().join("main.wesl"), "// theirs\n").unwrap();
        driver.type_text("// edited\n");
        driver.save();
        driver.pump();
        assert!(matches!(
            driver.app.prompt,
            Some(Prompt::Conflict { deleted: false, .. })
        ));

        driver.type_text("// more\n");
        driver.answer(Answer::Overwrite);
        let write = in_flight(&driver).unwrap();
        assert_eq!(write.text, driver.app.document.content());
        assert_eq!(write.text, format!("// edited\n// more\n{MAIN}"));
        assert_eq!(write.expect, crate::core::folder::Expect::Anything);

        driver.pump();
        assert_eq!(
            fs::read_to_string(dir.path().join("main.wesl")).unwrap(),
            write.text
        );
        assert!(!driver.app.document.is_dirty());
        assert!(driver.app.prompt.is_none());
    }

    #[test]
    fn after_a_delete_the_next_save_expects_missing() {
        let dir = folder();
        let mut driver = Driver::new();
        driver.enter(&dir);

        fs::remove_file(dir.path().join("main.wesl")).unwrap();
        driver.changes(vec![Change::Removed(path("main.wesl"))]);
        assert!(matches!(
            driver.app.prompt,
            Some(Prompt::ChangedOnDisk { deleted: true, .. })
        ));
        driver.answer(Answer::KeepMine);
        assert!(driver.app.document.is_dirty());

        driver.save();
        assert_eq!(
            in_flight(&driver).unwrap().expect,
            crate::core::folder::Expect::Missing
        );
        driver.pump();
        assert_eq!(
            fs::read_to_string(dir.path().join("main.wesl")).unwrap(),
            MAIN
        );
        assert!(!driver.app.document.is_dirty());
    }

    #[test]
    fn escape_cancels_a_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let mut driver = Driver::new();
        let _ = dirty_project(&mut driver, &dir);
        let edited = driver.app.document.content();
        driver.update(Message::NewProject);
        assert!(driver.app.prompt.is_some());

        driver.update(Message::Escape);

        assert!(driver.app.prompt.is_none());
        assert_eq!(driver.app.document.content(), edited);
        assert!(driver.app.is_dirty());
    }

    #[test]
    fn a_prompt_refuses_other_file_actions() {
        let dir = tempfile::tempdir().unwrap();
        let mut driver = Driver::new();
        let _ = dirty_project(&mut driver, &dir);
        driver.update(Message::NewProject);

        driver.update(Message::CloseRequested);
        driver.update(Message::OpenFolder);
        driver.save();

        assert!(matches!(
            driver.app.prompt,
            Some(Prompt::Unsaved {
                then: Pending::NewProject
            })
        ));
        assert!(!driver.app.opener.is_busy());
        assert!(!driver.app.document.is_loading());
    }

    #[test]
    fn the_header_marks_unsaved_work() {
        let dir = tempfile::tempdir().unwrap();
        let mut driver = Driver::new();
        assert!(!driver.app.is_dirty());

        driver.update(Message::Uniforms(uniforms::Message::AddTime));
        assert!(driver.app.is_dirty(), "a layout change dirties the project");

        let _ = dirty_project(&mut driver, &dir);
        assert!(driver.app.is_dirty());
    }

    #[test]
    fn open_a_type_save_open_b_undo_leaves_b_unchanged() {
        let dir = folder();
        let mut driver = Driver::new();
        driver.enter(&dir);

        driver.type_text("// a\n");
        driver.save();
        driver.pump();
        driver.explore(explorer::Message::Open(path("other.wesl")));
        assert_eq!(driver.app.document.content(), OTHER);

        driver.update(Message::Document(document::Message::Undo));
        assert_eq!(driver.app.document.content(), OTHER);
        assert!(!driver.app.document.is_dirty());
    }

    #[test]
    fn set_main_from_the_explorer_recompiles() {
        let dir = folder();
        let mut driver = Driver::new();
        driver.enter(&dir);
        assert_eq!(driver.app.viewer.error(), None);

        driver.explore(explorer::Message::SetMain(path("other.wesl")));
        let error = driver
            .app
            .viewer
            .error()
            .expect("other.wesl has no mainBulin");
        assert!(error.contains("mainBulin"), "{error}");
        driver.pump();
        let settings = fs::read_to_string(dir.path().join(SETTINGS)).unwrap();
        assert_eq!(
            Settings::from_json(&settings).unwrap().main,
            Some(path("other.wesl"))
        );

        driver.explore(explorer::Message::SetMain(path("main.wesl")));
        assert_eq!(driver.app.viewer.error(), None);
    }

    #[test]
    fn folder_exits_queue_settings_before_the_drain() {
        let next = folder();
        let json = include_str!("../assets/test_project.bulin");
        let exits = [
            Message::CloseFolder,
            Message::Opener(folder::opener::Message::FolderPicked(Some(
                next.path().to_owned(),
            ))),
            Message::Opener(folder::opener::Message::ProjectPicked(Ok((
                file::Name::from("/projects/test.bulin"),
                Arc::new(json.to_owned()),
            )))),
        ];
        let radius = |value| {
            let mut uniforms = Uniforms::default();
            uniforms.add("radius", Type::Float(value)).unwrap();
            uniforms
        };

        for exit in exits {
            let dir = folder();
            let settings = Settings::new(Some(path("main.wesl")), radius(0.5), None);
            fs::write(dir.path().join(SETTINGS), settings.to_json()).unwrap();
            let mut driver = Driver::new();
            driver.enter(&dir);
            driver.update(Message::Uniforms(uniforms::Message::AddTime));
            driver.pump();
            driver.update(Message::Uniforms(uniforms::Message::set(
                "radius",
                Type::Float(0.7),
            )));
            let clock = driver.app.uniforms.clock();

            driver.update(exit.clone());
            assert!(driver.app.draining.is_some(), "{exit:?}");
            assert!(
                driver.is_folder(),
                "the folder stays open until its writes are done: {exit:?}"
            );
            assert_eq!(
                in_flight(&driver).map(|write| write.path),
                Some(path(SETTINGS)),
                "{exit:?}"
            );

            assert_eq!(driver.pump(), [SETTINGS], "{exit:?}");
            assert!(driver.app.draining.is_none(), "{exit:?}");
            assert!(
                !driver.is_folder() || driver.app.sessions == 3,
                "the exit goes on: {exit:?}"
            );
            let saved = fs::read_to_string(dir.path().join(SETTINGS)).unwrap();
            let saved = Settings::from_json(&saved).unwrap();
            assert_eq!(saved.uniforms, radius(0.7), "{exit:?}");
            assert_eq!(saved.clock, clock, "{exit:?}");
            assert!(clock.is_some());
        }
    }

    #[test]
    fn project_mode_open_project_goes_through_the_opener() {
        let mut driver = Driver::new();
        let json = include_str!("../assets/test_project_2.bulin");

        driver.update(Message::Opener(folder::opener::Message::ProjectPicked(Ok(
            (
                file::Name::from("/projects/test.bulin"),
                Arc::new(json.to_owned()),
            ),
        ))));

        assert!(matches!(
            driver.app.mode,
            Mode::Project { file: Some(_), .. }
        ));
        assert_eq!(
            driver.app.document.content(),
            Project::from_json(json).unwrap().shader
        );
        assert_eq!(driver.app.viewer.error(), None);

        driver.update(Message::Opener(folder::opener::Message::ProjectPicked(Ok(
            (
                file::Name::from("/projects/broken.bulin"),
                Arc::new("not json".to_owned()),
            ),
        ))));
        assert!(
            driver
                .app
                .notice
                .as_deref()
                .is_some_and(|notice| notice.starts_with("Could not open the project"))
        );
    }

    #[test]
    fn a_restart_drops_old_generations_without_panicking() {
        let dir = folder();
        let mut driver = Driver::new();
        driver.enter(&dir);
        let stale = driver.app.generation;
        // Another client's event, which the running client would assert isn't its own.
        let (_other, mut other) = analyzer::Analyzer::start(analyzer::Workspace::Scratch);
        let event = driver.runtime.block_on(other.next()).unwrap();

        driver.update(Message::CloseFolder);
        driver.pump();
        assert!(!driver.is_folder());
        assert_ne!(driver.app.generation, stale);
        let mut events =
            iced_runtime::task::into_stream(std::mem::replace(&mut driver.task, Task::none()))
                .expect("leaving the folder streams the scratch analyzer's events");

        driver.update(Message::Analyzer(stale, event));
        driver.type_text("@");
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let status = driver
                .app
                .document
                .status(driver.app.analyzer.as_ref().unwrap());
            if status.starts_with(|c: char| c.is_ascii_digit()) && !status.starts_with("0 errors") {
                break;
            }
            assert!(Instant::now() < deadline, "no error within 30 s: {status}");
            match events.next().now_or_never() {
                Some(Some(iced_runtime::Action::Output(message))) => driver.update(message),
                Some(Some(_)) => {}
                Some(None) => panic!("the analyzer's events ended"),
                None => thread::sleep(Duration::from_millis(10)),
            }
        }
    }

    /// Renames the folder entry `from` to `name`, in the same directory, through the explorer.
    fn rename(driver: &mut Driver, from: &str, name: &str) {
        driver.explore(explorer::Message::Rename(path(from)));
        driver.explore(explorer::Message::Edited(name.to_owned()));
        driver.explore(explorer::Message::Submit);
    }

    /// Deletes the folder entry `file` through the explorer, answering the question with `answer`.
    fn delete(driver: &mut Driver, file: &str, answer: Answer) {
        driver.explore(explorer::Message::Delete(path(file)));
        assert!(
            matches!(&driver.app.prompt, Some(Prompt::Delete { path: asked }) if *asked == path(file))
        );
        driver.answer(answer);
    }

    /// Lists the folder's root at `dir` for the explorer, as opening the folder does.
    fn list_root(driver: &mut Driver, dir: &TempDir) {
        let handle = folder::Handle::new(dir.path().to_owned());
        let result = driver.runtime.block_on(folder::scan_dir(handle, None));
        driver.update(Message::Folder(folder::Message::Listed {
            session: driver.app.sessions,
            dir: None,
            result,
        }));
    }

    fn state(driver: &Driver) -> &folder::State {
        let Mode::Folder(state) = &driver.app.mode else {
            panic!("a folder is open");
        };
        state
    }

    fn is_at(driver: &Driver, file: &str) -> bool {
        matches!(driver.app.document.location(), document::Location::Folder(at) if *at == path(file))
    }

    #[test]
    fn renaming_the_document_moves_it() {
        let dir = folder();
        let mut driver = Driver::new();
        driver.enter(&dir);
        driver.explore(explorer::Message::Open(path("other.wesl")));
        driver.type_text("x");

        rename(&mut driver, "other.wesl", "moved.wesl");
        assert!(matches!(
            state(&driver).in_flight(),
            Some(folder::Op::Rename { .. })
        ));
        // A save of the old path, queued behind the rename, finds the file gone.
        driver.save();
        driver.pump();

        assert!(is_at(&driver, "moved.wesl"));
        assert_eq!(driver.app.document.content(), format!("x{OTHER}"));
        assert!(driver.app.document.is_dirty());
        assert_eq!(driver.app.document.label(), "moved.wesl");
        assert!(!dir.path().join("other.wesl").exists());
        assert_eq!(
            fs::read_to_string(dir.path().join("moved.wesl")).unwrap(),
            OTHER
        );
        assert!(driver.app.prompt.is_none());
        assert!(driver.app.notice.is_some());
    }

    #[test]
    fn a_rename_echo_before_its_completion_doesnt_touch_the_document() {
        let dir = folder();
        let mut driver = Driver::new();
        driver.enter(&dir);
        driver.explore(explorer::Message::Open(path("other.wesl")));

        rename(&mut driver, "other.wesl", "moved.wesl");
        driver.changes(vec![
            Change::Removed(path("other.wesl")),
            changed("moved.wesl", OTHER),
        ]);
        assert!(driver.app.prompt.is_none());
        assert_eq!(driver.app.notice, None);
        assert!(is_at(&driver, "other.wesl"));
        assert!(!driver.app.document.is_dirty());

        driver.pump();
        assert!(is_at(&driver, "moved.wesl"));
        assert!(driver.app.prompt.is_none());
        assert_eq!(driver.app.notice, None);
    }

    #[test]
    fn renaming_the_main_file_updates_settings() {
        let dir = folder();
        let mut driver = Driver::new();
        driver.enter(&dir);

        rename(&mut driver, "main.wesl", "start.wesl");
        assert!(driver.pump().contains(&SETTINGS.to_owned()));

        let settings = fs::read_to_string(dir.path().join(SETTINGS)).unwrap();
        assert_eq!(
            Settings::from_json(&settings).unwrap().main,
            Some(path("start.wesl"))
        );
        assert!(is_at(&driver, "start.wesl"));
        assert!(state(&driver).files().contains_key(&path("start.wesl")));
        assert_eq!(driver.app.viewer.error(), None);
    }

    #[test]
    fn renaming_a_directory_moves_the_document_inside_it() {
        let dir = folder();
        fs::create_dir(dir.path().join("shaders")).unwrap();
        fs::write(dir.path().join("shaders/noise.wesl"), OTHER).unwrap();
        let mut driver = Driver::new();
        driver.enter(&dir);
        driver.changes(vec![changed("shaders/noise.wesl", OTHER)]);
        driver.explore(explorer::Message::Open(path("shaders/noise.wesl")));
        assert_eq!(driver.app.document.content(), OTHER);

        rename(&mut driver, "shaders", "effects");
        driver.pump();

        assert!(is_at(&driver, "effects/noise.wesl"));
        assert_eq!(driver.app.document.content(), OTHER);
        assert!(dir.path().join("effects/noise.wesl").exists());
        assert!(!dir.path().join("shaders").exists());
        assert!(driver.app.prompt.is_none());
        assert_eq!(driver.app.notice, None);
    }

    #[test]
    fn delete_asks_first() {
        let dir = folder();
        let mut driver = Driver::new();
        driver.enter(&dir);
        list_root(&mut driver, &dir);
        let other = Entry {
            kind: Kind::File,
            path: path("other.wesl"),
        };
        assert!(state(&driver).listing(None).unwrap().contains(&other));

        delete(&mut driver, "other.wesl", Answer::Cancel);
        assert!(driver.app.prompt.is_none());
        assert!(driver.pump().is_empty());
        assert!(dir.path().join("other.wesl").exists());

        delete(&mut driver, "other.wesl", Answer::Delete);
        assert_eq!(driver.pump(), ["other.wesl"]);
        assert!(!dir.path().join("other.wesl").exists());
        assert!(!state(&driver).files().contains_key(&path("other.wesl")));
        assert!(!state(&driver).listing(None).unwrap().contains(&other));
        assert_eq!(driver.app.notice, None);
    }

    #[test]
    fn deleting_the_main_file_clears_it() {
        let dir = folder();
        let mut driver = Driver::new();
        driver.enter(&dir);

        delete(&mut driver, "main.wesl", Answer::Delete);
        driver.pump();

        let settings = fs::read_to_string(dir.path().join(SETTINGS)).unwrap();
        assert_eq!(Settings::from_json(&settings).unwrap().main, None);
        assert_eq!(
            driver.app.viewer.error(),
            Some("This folder has no main file. Set one in the explorer.")
        );
        assert!(matches!(
            driver.app.document.location(),
            document::Location::Untitled
        ));
        assert!(driver.app.prompt.is_none());
    }

    #[test]
    fn deleting_the_open_document_closes_it() {
        let dir = folder();
        let mut driver = Driver::new();
        driver.enter(&dir);
        driver.explore(explorer::Message::Open(path("other.wesl")));
        driver.type_text("x");

        delete(&mut driver, "other.wesl", Answer::Delete);
        driver.pump();

        assert!(matches!(
            driver.app.document.location(),
            document::Location::Untitled
        ));
        assert_eq!(driver.app.document.content(), "");
        assert!(!driver.app.is_dirty());
        assert!(driver.app.prompt.is_none());
        assert_eq!(driver.app.viewer.error(), None, "the main file still runs");
    }

    #[test]
    fn creating_an_existing_file_reports_it() {
        let dir = folder();
        let mut driver = Driver::new();
        driver.enter(&dir);
        list_root(&mut driver, &dir);

        driver.explore(explorer::Message::NewFile(None));
        driver.explore(explorer::Message::Edited("other.wesl".to_owned()));
        driver.explore(explorer::Message::Submit);
        driver.pump();
        let notice = driver.app.notice.take().unwrap();
        assert!(notice.contains("other.wesl already exists"), "{notice}");
        assert_eq!(
            fs::read_to_string(dir.path().join("other.wesl")).unwrap(),
            OTHER
        );

        driver.explore(explorer::Message::NewFile(None));
        driver.explore(explorer::Message::Submit);
        driver.explore(explorer::Message::NewFolder(None));
        driver.explore(explorer::Message::Edited("shaders".to_owned()));
        driver.explore(explorer::Message::Submit);
        assert_eq!(driver.pump(), ["untitled.wesl", "shaders"]);

        assert_eq!(
            fs::read_to_string(dir.path().join("untitled.wesl")).unwrap(),
            ""
        );
        assert!(dir.path().join("shaders").is_dir());
        let listing = state(&driver).listing(None).unwrap();
        for (kind, file) in [(Kind::File, "untitled.wesl"), (Kind::Dir, "shaders")] {
            let entry = Entry {
                kind,
                path: path(file),
            };
            assert!(listing.contains(&entry), "{listing:?}");
        }
        assert_eq!(driver.app.notice, None);
    }
}
