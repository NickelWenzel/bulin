pub use bulin_core as core;

mod action;
mod analyzer;
mod file;
mod layout;
mod menu;
mod shader;
mod uniforms;
mod viewer;

use iced::keyboard::key;
use iced::time::Instant;
use iced::widget::{button, center, column, container, mouse_area, opaque, stack, text};
use iced::{Color, Element, Event, Font, Length, Subscription, Task, Theme, event, keyboard};
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
}

impl Application {
    pub fn new() -> (Self, Task<Message>) {
        let (mut analyzer, events) = analyzer::Analyzer::start();
        let project = core::Project::untitled();
        let mut shader = shader::State::new(&project.shader);
        shader.open_lsp(&mut analyzer);
        analyzer.flush();
        let uniforms = uniforms::State::new(project.uniforms, project.clock);
        let viewer = viewer::State::new(&program(&shader, &uniforms));

        let application = Self {
            analyzer,
            shader,
            uniforms,
            viewer,
            layout: layout::State::new(),
            file: None,
            is_loading: false,
            show_menu: false,
            notice: None,
        };

        (application, Task::run(events, Message::Analyzer))
    }

    pub fn update(&mut self, message: Message, now: Instant) -> Task<Message> {
        let task = match message {
            Message::Shader(message) => {
                let action = self.shader.update(message, now, &mut self.analyzer);
                if let Some(shader::Instruction::Changed) = action.instruction {
                    self.viewer.load(&program(&self.shader, &self.uniforms));
                }
                action.task.map(Message::Shader)
            }
            Message::Uniforms(message) => {
                if let Some(uniforms::Instruction::Changed) = self.uniforms.update(message) {
                    self.viewer.load(&program(&self.shader, &self.uniforms));
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
                if self.is_loading {
                    Task::none()
                } else {
                    self.is_loading = true;
                    self.notice = None;

                    Task::perform(file::open_file(), Message::ProjectOpened)
                }
            }
            Message::NewProject => {
                if !self.is_loading {
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
                if self.is_loading {
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
                        file::save_file(file, project.to_json()),
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
                let mut tasks = Vec::new();
                for update in self.analyzer.client().receive(event) {
                    match update {
                        lsp::Update::Document(document) => {
                            let action = self.shader.apply_lsp(&mut self.analyzer, document);
                            if let Some(shader::Instruction::Changed) = action.instruction {
                                self.viewer.load(&program(&self.shader, &self.uniforms));
                            }
                            tasks.push(action.task.map(Message::Shader));
                        }
                        lsp::Update::Status(status) => tracing::info!("wgsl-analyzer: {status:?}"),
                        lsp::Update::Log(entry) if entry.is_shown() => {
                            tracing::info!("wgsl-analyzer: {}", entry.text())
                        }
                        lsp::Update::Log(entry) => {
                            tracing::debug!("wgsl-analyzer: {}", entry.text())
                        }
                        lsp::Update::Error(error) => tracing::warn!("wgsl-analyzer: {error}"),
                        // Renames only reach the open shader, so edits for other files never come.
                        lsp::Update::FileEdits(_)
                        | lsp::Update::Notification(_)
                        | lsp::Update::Trace(_) => {}
                    }
                }

                Task::batch(tasks)
            }
        };

        self.analyzer.flush();
        task
    }

    /// Replaces the shader and uniforms with `project`'s.
    fn load(&mut self, project: core::Project) {
        self.shader.close_lsp(&mut self.analyzer);
        self.shader = shader::State::new(&project.shader);
        self.shader.open_lsp(&mut self.analyzer);
        self.uniforms = uniforms::State::new(project.uniforms, project.clock);
        self.viewer.load(&program(&self.shader, &self.uniforms));
    }

    pub fn view(&self) -> Element<'_, Message> {
        let panes = self
            .layout
            .view(|_, pane, _| match pane {
                layout::PaneContent::Editor => {
                    let file = self
                        .shader
                        .filename_display_text()
                        .unwrap_or_else(|| String::from("New file"));
                    let title = match &self.notice {
                        Some(notice) => format!("{file} · {notice}"),
                        None => file,
                    };
                    let editors = column![
                        self.uniforms.view().map(Message::Uniforms),
                        self.shader.view(&self.analyzer).map(Message::Shader),
                    ];

                    (container(editors).into(), Some(title))
                }
                layout::PaneContent::Viewer => (self.viewer.view().into(), None),
            })
            .on_drag(|e| Message::Layout(layout::Message::Dragged(e)))
            .on_resize(10, |e| Message::Layout(layout::Message::Resized(e)));

        let content = column!(
            menu_icon(),
            container(panes).width(Length::Fill).height(Length::Fill),
        )
        .into();

        if self.show_menu {
            modal(content, menu::view().map(Message::Menu), Message::CloseMenu)
        } else {
            content
        }
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

        Subscription::batch([
            self.shader.subscription().map(Message::Shader),
            self.uniforms.subscription().map(Message::Uniforms),
            escape,
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

fn modal<'a>(
    base: impl Into<Element<'a, Message>>,
    content: impl Into<Element<'a, Message>>,
    on_blur: Message,
) -> Element<'a, Message> {
    stack![
        base.into(),
        opaque(
            mouse_area(center(opaque(content)).style(|_theme| {
                container::Style {
                    background: Some(
                        Color {
                            a: 0.8,
                            ..Color::BLACK
                        }
                        .into(),
                    ),
                    ..container::Style::default()
                }
            }))
            .on_press(on_blur)
        )
    ]
    .into()
}
