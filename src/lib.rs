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
    keyboard,
};
use matcha_iced::lsp;

use std::sync::Arc;

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
    /// What to do once the folder's queued writes are done.
    draining: Option<Pending>,
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
    /// A project, saved in `file` once it has one.
    Project {
        file: Option<file::Name>,
    },
    Folder(Box<folder::State>),
}

/// What waits for the folder's writes before it leaves the folder.
enum Pending {
    OpenFolder(folder::Handle),
    /// A project file and its text, which parses.
    OpenProject(file::Name, Arc<String>),
    CloseFolder,
}

/// Why the app can't take another file action now.
enum Busy {
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
    ProjectSaved(Result<file::Name, file::Error>),
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
            mode: Mode::Project { file: None },
            opener: folder::opener::Opener::default(),
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
        match message {
            Message::Document(message) => {
                // A document dialog answering after a project switch would land in the new project.
                let file_action = matches!(
                    message,
                    document::Message::NewFile
                        | document::Message::OpenFile
                        | document::Message::SaveFile
                        | document::Message::SaveFileAs
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
                        if let Mode::Folder(state) = &mut self.mode {
                            state.edited(self.uniforms.uniforms(), self.uniforms.clock(), now);
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
                            Mode::Project { .. } => task,
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
                let action = state.update(
                    message,
                    now,
                    &mut self.document,
                    running(&mut self.analyzer),
                    &self.settings,
                    &mut self.notice,
                );
                let task = action.task.map(Message::Folder);
                match action.instruction {
                    Some(folder::Instruction::ProgramChanged) => self.program_changed(),
                    Some(folder::Instruction::Drained) | None => {}
                }
                // A completion can finish the queue whatever else it reports.
                Task::batch([task, self.drain()])
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
                // The window stays usable while the user picks, so the folder may have changed.
                if self.unsaved() {
                    return task;
                }
                if self.draining.is_some() {
                    self.notice = Some(DRAINING.to_owned());
                    return task;
                }
                // The settings write joins the queue before the drain waits for it.
                let leave = match &mut self.mode {
                    Mode::Folder(state) => state
                        .leave(self.uniforms.uniforms(), self.uniforms.clock())
                        .map(Message::Folder),
                    Mode::Project { .. } => Task::none(),
                };
                self.draining = Some(then);
                Task::batch([task, leave, self.drain()])
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
                Some(Busy::Dialog) if self.opener.is_busy() => {
                    self.opener.close();
                    Task::none()
                }
                Some(Busy::Dialog | Busy::Draining) | None => self.update(Message::CloseModal, now),
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
                        return self.update(
                            Message::Document(document::Message::Editor(
                                matcha_iced::Event::OpenFind,
                            )),
                            now,
                        );
                    }
                    menu::Message::Replace => {
                        return self.update(
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
                Task::batch([operation::focus(EDITOR), self.update(message, now)])
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
            Message::OpenProject | Message::OpenFolder => {
                if self.refuse() || self.unsaved() {
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
                if !matches!(self.mode, Mode::Folder(_)) || self.refuse() || self.unsaved() {
                    return Task::none();
                }
                self.notice = None;
                let leave = match &mut self.mode {
                    Mode::Folder(state) => state
                        .leave(self.uniforms.uniforms(), self.uniforms.clock())
                        .map(Message::Folder),
                    Mode::Project { .. } => Task::none(),
                };
                self.draining = Some(Pending::CloseFolder);
                Task::batch([leave, self.drain()])
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
                if matches!(self.mode, Mode::Project { .. }) && !self.refuse() {
                    self.mode = Mode::Project { file: None };
                    self.notice = None;
                    self.load(core::Project::untitled());
                }
                Task::none()
            }
            Message::SaveProject | Message::SaveProjectAs => {
                if self.refuse() {
                    return Task::none();
                }
                let Mode::Project { file } = &self.mode else {
                    return Task::none();
                };
                let file = match message {
                    Message::SaveProject => file.clone(),
                    _ => None,
                };
                self.is_loading = true;
                self.notice = None;
                let project = core::Project::new(
                    self.document.content(),
                    self.uniforms.uniforms().clone(),
                    self.uniforms.clock(),
                );

                Task::perform(
                    file::save_file(file, "project.bulin", project.to_json()),
                    Message::ProjectSaved,
                )
            }
            Message::ProjectSaved(result) => {
                self.is_loading = false;

                match result {
                    Ok(path) => {
                        if let Mode::Project { file } = &mut self.mode {
                            *file = Some(path);
                        }
                    }
                    Err(file::Error::DialogClosed) => {}
                    Err(error) => {
                        self.notice = Some(format!("Could not save the project: {error}"))
                    }
                }

                Task::none()
            }
            Message::Analyzer(generation, event) => {
                // The client asserts an event is its own, and an event doesn't say whose it is.
                if generation != self.generation {
                    return Task::none();
                }
                let action = self.document.lsp(running(&mut self.analyzer), event);
                match action.instruction {
                    Some(document::Instruction::Changed) => self.program_changed(),
                    Some(document::Instruction::Save { .. }) | None => {}
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
        if self.is_loading || self.document.is_loading() || self.opener.is_busy() {
            Some(Busy::Dialog)
        } else if self.draining.is_some() {
            Some(Busy::Draining)
        } else {
            None
        }
    }

    /// Whether a file action must wait, telling the user why unless a dialog shows it.
    fn refuse(&mut self) -> bool {
        match self.busy() {
            Some(Busy::Dialog) => true,
            Some(Busy::Draining) => {
                self.notice = Some(DRAINING.to_owned());
                true
            }
            None => false,
        }
    }

    /// Whether the folder's open file has unsaved changes, which leaving the folder would lose,
    /// telling the user so.
    fn unsaved(&mut self) -> bool {
        matches!(self.mode, Mode::Folder(_)) && folder::unsaved(&self.document, &mut self.notice)
    }

    /// Runs what waits for the folder's writes, once they are done.
    fn drain(&mut self) -> Task<Message> {
        if let Mode::Folder(state) = &self.mode
            && !state.is_idle()
        {
            return Task::none();
        }
        match self.draining.take() {
            Some(Pending::OpenFolder(handle)) => {
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
            Some(Pending::OpenProject(name, text)) => match core::Project::from_json(&text) {
                Ok(project) => {
                    let events = self.leave();
                    self.mode = Mode::Project { file: Some(name) };
                    self.load(project);
                    events
                }
                Err(error) => {
                    self.notice = Some(format!("Could not open the project: {error}"));
                    Task::none()
                }
            },
            Some(Pending::CloseFolder) => {
                let events = self.leave();
                self.mode = Mode::Project { file: None };
                self.load(core::Project::untitled());
                events
            }
            None => Task::none(),
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

    /// Replaces the shader and uniforms with `project`'s.
    fn load(&mut self, project: core::Project) {
        self.document.detach(running(&mut self.analyzer));
        self.document = document::State::new(&project.shader, &self.settings);
        self.document.attach(running(&mut self.analyzer));
        self.uniforms = uniforms::State::new(project.uniforms, project.clock);
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

        let document = self.document.title();
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

        // The content keeps its place in the widget tree whether a modal is open or not, so
        // opening one doesn't reset the editor's document, the combo box or the panes.
        let layer = self.modal.as_ref().map(|layer| match layer {
            Modal::Menu => menu::view(&self.mode, self.settings.editor.find).map(Message::Menu),
            Modal::Settings(state) => state.view(&self.settings).map(Message::Settings),
        });

        stack![content]
            .push(layer.map(|layer| modal(layer, Message::CloseModal)))
            .push(
                self.opener
                    .view()
                    .map(|opener| modal(opener.map(Message::Opener), Message::Escape)),
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

        // Escape closes the library or a modal even while the editor holds focus and captures
        // keys.
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

/// `content` over a dimmed backdrop that swallows input and sends `on_blur` when clicked.
fn modal<'a>(content: impl Into<Element<'a, Message>>, on_blur: Message) -> Element<'a, Message> {
    opaque(
        mouse_area(center(opaque(content)).style(|_theme| container::Style {
            background: Some(color!(0x000000, 0.8).into()),
            ..container::Style::default()
        }))
        .on_press(on_blur),
    )
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
        Application, DRAINING, Message, Modal, Mode, analyzer, document, explorer, folder, uniforms,
    };
    use crate::core;
    use crate::core::folder::{Change, Path, SETTINGS, Settings};
    use crate::core::uniform::Type;
    use crate::core::{Project, Uniforms};
    use crate::file;
    use crate::menu;
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

        /// Runs the folder's writes until none is left, returning the paths written.
        fn pump(&mut self) -> Vec<String> {
            let mut written = Vec::new();
            loop {
                let Mode::Folder(state) = &self.app.mode else {
                    return written;
                };
                let Some(write) = state.in_flight().cloned() else {
                    return written;
                };
                written.push(write.path.as_str().to_owned());
                let result = self.runtime.block_on(folder::write(
                    write.handle,
                    write.path.clone(),
                    write.text.clone(),
                    write.expect,
                ));
                self.update(Message::Folder(folder::Message::Wrote {
                    session: self.app.sessions,
                    path: write.path,
                    text: write.text,
                    result,
                }));
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
        assert!(matches!(driver.app.mode, Mode::Project { file: Some(_) }));
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

    #[test]
    fn dirty_folder_refuses_open_and_close() {
        let dir = folder();
        let other = folder();
        let mut driver = Driver::new();
        driver.enter(&dir);
        driver.type_text(" ");

        driver.update(Message::CloseFolder);
        assert!(driver.app.notice.is_some());
        assert!(driver.app.draining.is_none());
        driver.app.notice = None;
        driver.update(Message::OpenFolder);
        assert!(driver.app.notice.is_some());
        assert!(!driver.app.opener.is_busy());
        driver.app.notice = None;
        driver.update(Message::OpenProject);
        assert!(driver.app.notice.is_some());

        driver.open(&other);
        assert!(driver.app.draining.is_none());
        assert_eq!(driver.app.sessions, 2);
        assert!(driver.is_folder());
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
            let Mode::Folder(state) = &driver.app.mode else {
                panic!("the folder stays open until its writes are done: {exit:?}");
            };
            assert_eq!(
                state.in_flight().map(|write| write.path.as_str()),
                Some(SETTINGS),
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

        assert!(matches!(driver.app.mode, Mode::Project { file: Some(_) }));
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
}
