pub use bulin_core as core;

mod action;
mod analyzer;
mod control;
mod file;
mod layout;
mod menu;
mod settings;
mod shader;
mod uniforms;
mod viewer;

use iced::keyboard::key;
use iced::time::Instant;
use iced::widget::{
    button, center, column, container, mouse_area, opaque, operation, row, stack, text,
};
use iced::{
    Center, Element, Event, Fill, Font, Length, Subscription, Task, Theme, color, event, keyboard,
};
use matcha_iced::lsp;

use std::sync::Arc;

/// matcha's default id for the editor, which takes focus back when a modal closes.
const EDITOR: &str = "matcha-editor";

pub struct Application {
    analyzer: analyzer::Analyzer,
    shader: shader::State,
    uniforms: uniforms::State,
    viewer: viewer::State,
    layout: layout::State,
    file: Option<file::Name>,
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
    /// Why the last project action failed, until the next one.
    notice: Option<String>,
}

/// What the layer over the content shows.
enum Modal {
    Menu,
    Settings(settings::State),
}

#[derive(Debug, Clone)]
pub enum Message {
    Shader(shader::Message),
    Uniforms(uniforms::Message),
    Layout(layout::Message),
    ShowMenu,
    CloseModal,
    Menu(menu::Message),
    Settings(settings::Message),
    OpenProject,
    NewProject,
    ProjectOpened(Result<(file::Name, Arc<String>), file::Error>),
    SaveProject,
    SaveProjectAs,
    ProjectSaved(Result<file::Name, file::Error>),
    Analyzer(lsp::client::Event),
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
        let (mut analyzer, events) = analyzer::Analyzer::start();
        let project = core::Project::untitled();
        let mut shader = shader::State::new(&project.shader, &settings);
        shader.attach(&mut analyzer);
        let uniforms = uniforms::State::new(project.uniforms, project.clock);
        let program = program(&shader, &uniforms);
        analyzer.set_uniforms(&program.uniforms_package());
        let viewer = viewer::State::new(&program);

        let application = Self {
            analyzer,
            shader,
            uniforms,
            viewer,
            layout: layout::State::new(),
            file: None,
            is_loading: false,
            modal: None,
            settings,
            saving_settings: false,
            settings_stale: false,
            settings_unreadable: false,
            notice: None,
        };

        (application, Task::run(events, Message::Analyzer))
    }

    pub fn update(&mut self, message: Message, now: Instant) -> Task<Message> {
        match message {
            Message::Shader(message) => {
                // A shader dialog answering after a project switch would land in the new project.
                let file_action = matches!(
                    message,
                    shader::Message::NewFile
                        | shader::Message::OpenFile
                        | shader::Message::SaveFile
                        | shader::Message::SaveFileAs
                );
                if file_action && self.is_loading {
                    return Task::none();
                }
                let action = self
                    .shader
                    .update(message, now, &mut self.analyzer, &self.settings);
                if let Some(shader::Instruction::Changed) = action.instruction {
                    self.program_changed();
                }
                action.task.map(Message::Shader)
            }
            Message::Uniforms(message) => {
                let action = self.uniforms.update(message);
                match action.instruction {
                    Some(uniforms::Instruction::Values) => self.viewer.set_values(
                        core::program::bytes(self.uniforms.time(), self.uniforms.uniforms()),
                    ),
                    Some(uniforms::Instruction::Layout) => self.program_changed(),
                    None => {}
                }
                action.task.map(Message::Uniforms)
            }
            Message::Layout(message) => {
                self.layout.update(message);
                Task::none()
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
                            Message::Shader(shader::Message::Editor(matcha_iced::Event::OpenFind)),
                            now,
                        );
                    }
                    menu::Message::Replace => {
                        return self.update(
                            Message::Shader(shader::Message::Editor(
                                matcha_iced::Event::OpenReplace,
                            )),
                            now,
                        );
                    }
                    menu::Message::OpenFile => Message::Shader(shader::Message::OpenFile),
                    menu::Message::NewFile => Message::Shader(shader::Message::NewFile),
                    menu::Message::SaveFile => Message::Shader(shader::Message::SaveFile),
                    menu::Message::SaveFileAs => Message::Shader(shader::Message::SaveFileAs),
                    menu::Message::Undo => Message::Shader(shader::Message::Undo),
                    menu::Message::Redo => Message::Shader(shader::Message::Redo),
                    menu::Message::OpenProject => Message::OpenProject,
                    menu::Message::NewProject => Message::NewProject,
                    menu::Message::SaveProject => Message::SaveProject,
                    menu::Message::SaveProjectAs => Message::SaveProjectAs,
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
                        self.shader.apply(&self.settings, now).map(Message::Shader),
                        self.settings_changed(),
                    ]),
                    Some(settings::Instruction::Previewed) => Task::batch([
                        task,
                        self.shader.apply(&self.settings, now).map(Message::Shader),
                    ]),
                    Some(settings::Instruction::Close) => {
                        self.modal = None;
                        Task::batch([task, operation::focus(EDITOR)])
                    }
                    None => task,
                }
            }
            Message::OpenProject => {
                if self.is_loading || self.shader.is_loading() {
                    Task::none()
                } else {
                    self.is_loading = true;
                    self.notice = None;

                    Task::perform(file::open_file(), Message::ProjectOpened)
                }
            }
            Message::NewProject => {
                if !self.is_loading && !self.shader.is_loading() {
                    self.file = None;
                    self.notice = None;
                    self.load(core::Project::untitled());
                }
                Task::none()
            }
            Message::ProjectOpened(result) => {
                self.is_loading = false;

                let project = match result {
                    Err(file::Error::DialogClosed) => Err(None),
                    Err(error) => Err(Some(error.to_string())),
                    Ok((path, json)) => core::Project::from_json(&json)
                        .map(|project| (path, project))
                        .map_err(|error| Some(error.to_string())),
                };
                match project {
                    Ok((path, project)) => {
                        self.file = Some(path);
                        self.load(project);
                    }
                    Err(error) => {
                        self.notice =
                            error.map(|error| format!("Could not open the project: {error}"));
                    }
                }
                Task::none()
            }
            Message::SaveProject | Message::SaveProjectAs => {
                if self.is_loading || self.shader.is_loading() {
                    Task::none()
                } else {
                    self.is_loading = true;
                    self.notice = None;
                    let file = match message {
                        Message::SaveProject => self.file.clone(),
                        _ => None,
                    };
                    let project = core::Project::new(
                        self.shader.content(),
                        self.uniforms.uniforms().clone(),
                        self.uniforms.clock(),
                    );

                    Task::perform(
                        file::save_file(file, "project.bulin", project.to_json()),
                        Message::ProjectSaved,
                    )
                }
            }
            Message::ProjectSaved(result) => {
                self.is_loading = false;

                match result {
                    Ok(path) => self.file = Some(path),
                    Err(file::Error::DialogClosed) => {}
                    Err(error) => {
                        self.notice = Some(format!("Could not save the project: {error}"))
                    }
                }

                Task::none()
            }
            Message::Analyzer(event) => {
                let action = self.shader.lsp(&mut self.analyzer, event);
                if let Some(shader::Instruction::Changed) = action.instruction {
                    self.program_changed();
                }
                action.task.map(Message::Shader)
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
                    self.shader.apply(&self.settings, now).map(Message::Shader),
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

    /// Shows the current program and gives the analyzer its uniforms package.
    fn program_changed(&mut self) {
        let program = program(&self.shader, &self.uniforms);
        self.viewer.load(&program);
        self.analyzer.set_uniforms(&program.uniforms_package());
    }

    /// Replaces the shader and uniforms with `project`'s.
    fn load(&mut self, project: core::Project) {
        self.shader.detach(&mut self.analyzer);
        self.shader = shader::State::new(&project.shader, &self.settings);
        self.shader.attach(&mut self.analyzer);
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
                        self.shader.view().map(Message::Shader),
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

        let title = [Some(self.shader.title().as_str()), self.notice.as_deref()]
            .into_iter()
            .chain([self.shader.failure()])
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        // Equal-width sides keep the title centered whatever the menu and status measure.
        let header = row![
            container(menu_icon()).width(Fill),
            text(title),
            container(text(self.shader.status(&self.analyzer)).size(14))
                .width(Fill)
                .align_right(Fill),
        ]
        .align_y(Center)
        .padding([0, 10]);

        let content = column![
            header,
            container(panes).width(Length::Fill).height(Length::Fill),
        ];

        // The content keeps its place in the widget tree whether a modal is open or not, so
        // opening one doesn't reset the editor's document, the combo box or the panes.
        let layer = self.modal.as_ref().map(|layer| match layer {
            Modal::Menu => menu::view(self.settings.editor.find).map(Message::Menu),
            Modal::Settings(state) => state.view(&self.settings).map(Message::Settings),
        });

        stack![content]
            .push(layer.map(|layer| modal(layer, Message::CloseModal)))
            .into()
    }

    pub fn theme(&self) -> Theme {
        self.shader.theme()
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

        // Escape closes a modal even while the shader editor holds focus and captures keys.
        let escape = event::listen_with(|event, _status, _window| match event {
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(key::Named::Escape),
                ..
            }) => Some(Message::CloseModal),
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

        Subscription::batch([
            self.shader.subscription().map(Message::Shader),
            self.uniforms.subscription().map(Message::Uniforms),
            escape,
            shortcuts,
        ])
    }
}

/// The program the viewer runs for the current shader and uniforms.
fn program(shader: &shader::State, uniforms: &uniforms::State) -> core::Program {
    core::Program::new(
        shader.content(),
        uniforms.uniforms().clone(),
        uniforms.time(),
    )
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
    use iced::advanced::widget::{Tree, tree};
    use iced::keyboard::{Key, Modifiers, key};
    use iced::time::Instant;

    use super::{Application, Message, Modal};
    use crate::core;
    use crate::menu;
    use crate::settings;

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
}
