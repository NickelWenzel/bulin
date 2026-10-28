pub use bulin_core as core;

mod action;
mod analyzer;
mod file;
mod layout;
mod menu;
mod settings;
mod shader;
mod uniforms;
mod viewer;

use iced::keyboard::key;
use iced::time::Instant;
use iced::widget::{button, center, column, container, mouse_area, opaque, row, stack, text};
use iced::{
    Center, Element, Event, Fill, Font, Length, Subscription, Task, Theme, color, event, keyboard,
};
use matcha_iced::lsp;

use std::sync::Arc;

pub struct Application {
    analyzer: analyzer::Analyzer,
    shader: shader::State,
    uniforms: uniforms::State,
    viewer: viewer::State,
    layout: layout::State,
    file: Option<file::Name>,
    is_loading: bool,
    show_menu: bool,
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

#[derive(Debug, Clone)]
pub enum Message {
    Shader(shader::Message),
    Uniforms(uniforms::Message),
    Layout(layout::Message),
    ShowMenu,
    CloseMenu,
    Menu(menu::Message),
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
            show_menu: false,
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
                match self.uniforms.update(message) {
                    Some(uniforms::Instruction::Values) => self.viewer.set_values(
                        core::program::bytes(self.uniforms.time(), self.uniforms.uniforms()),
                    ),
                    Some(uniforms::Instruction::Layout) => self.program_changed(),
                    None => {}
                }
                Task::none()
            }
            Message::Layout(message) => {
                self.layout.update(message);
                Task::none()
            }
            Message::ShowMenu => {
                self.show_menu = true;
                Task::none()
            }
            Message::CloseMenu => {
                self.show_menu = false;
                Task::none()
            }
            Message::Menu(message) => {
                self.show_menu = false;
                let message = match message {
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
                self.update(message, now)
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

        // The content keeps its place in the widget tree whether the menu is open or not, so
        // opening it doesn't reset the editor's focus, the combo box or the panes.
        stack![content]
            .push(
                self.show_menu
                    .then(|| modal(menu::view().map(Message::Menu), Message::CloseMenu)),
            )
            .into()
    }

    pub fn theme(&self) -> Theme {
        self.shader.theme()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        // Escape closes the menu even while the shader editor holds focus and captures keys.
        let escape = event::listen_with(|event, _status, _window| match event {
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(key::Named::Escape),
                ..
            }) => Some(Message::CloseMenu),
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

    use super::{Application, Message};
    use crate::core;

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

    #[test]
    fn opening_the_menu_keeps_the_content_state() {
        let (mut app, _) = Application::with_settings(core::Settings::default());
        let mut tree = Tree::empty();
        tree.diff(&mut app.view());
        let root = tree.tag;
        let content = state_address(&tree.children[0]);

        app.show_menu = true;
        tree.diff(&mut app.view());

        // A root of another kind would make iced rebuild the whole tree and drop its state.
        assert_eq!(tree.tag, root);
        assert!(content.is_some());
        assert_eq!(state_address(&tree.children[0]), content);
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
