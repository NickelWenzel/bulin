pub use bulin_core as core;

mod analyzer;
mod editor;
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
    editor: editor::Editor,
    viewer: viewer::State,
    layout: layout::State,
    file: Option<file::Name>,
    is_loading: bool,
    show_menu: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Editor(editor::Message),
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
        let mut editor = editor::Editor::new(core::program::DEFAULT_SOURCE);
        editor.open_lsp(&mut analyzer);
        analyzer.flush();
        let viewer = viewer::State::new(&editor.program());

        let application = Self {
            analyzer,
            editor,
            viewer,
            layout: layout::State::new(),
            file: None,
            is_loading: false,
            show_menu: false,
        };

        (application, Task::run(events, Message::Analyzer))
    }

    pub fn update(&mut self, message: Message, now: Instant) -> Task<Message> {
        let task = match message {
            Message::Editor(message) => match message {
                editor::Message::Changed => {
                    self.viewer.load(&self.editor.program());
                    Task::none()
                }
                _ => self
                    .editor
                    .update(message, now, &mut self.analyzer)
                    .map(Message::Editor),
            },
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
            Message::Menu(message) => match message {
                menu::Message::OpenProject => Task::done(Message::OpenProject),
                menu::Message::NewProject => Task::done(Message::NewProject),
                menu::Message::SaveProject => Task::done(Message::SaveProject),
                menu::Message::SaveProjectAs => Task::done(Message::SaveProjectAs),
                menu::Message::Editor(message) => Task::done(Message::Editor(message)),
            },
            Message::OpenProject => {
                if self.is_loading {
                    Task::none()
                } else {
                    self.is_loading = true;

                    Task::perform(file::open_file(), Message::ProjectOpened)
                }
            }
            Message::NewProject => {
                if self.is_loading {
                    Task::none()
                } else {
                    self.editor.close_lsp(&mut self.analyzer);
                    self.file = None;
                    self.editor = editor::Editor::new("");

                    self.editor
                        .update(editor::Message::ProjectOpened, now, &mut self.analyzer)
                        .map(Message::Editor)
                }
            }
            Message::ProjectOpened(result) => {
                self.is_loading = false;

                if let Ok((path, contents)) = result {
                    if let Ok(editor) = serde_json::from_str(&contents) {
                        self.editor.close_lsp(&mut self.analyzer);
                        self.file = Some(path);
                        self.editor = editor;
                        self.editor
                            .update(editor::Message::ProjectOpened, now, &mut self.analyzer)
                            .map(Message::Editor)
                    } else {
                        Task::none()
                    }
                } else {
                    Task::none()
                }
            }
            Message::SaveProject => {
                if self.is_loading {
                    Task::none()
                } else if let Ok(content) = serde_json::to_string(&self.editor) {
                    self.is_loading = true;
                    Task::perform(
                        file::save_file(self.file.clone(), content),
                        Message::ProjectSaved,
                    )
                } else {
                    Task::none()
                }
            }
            Message::SaveProjectAs => {
                if self.is_loading {
                    Task::none()
                } else if let Ok(content) = serde_json::to_string(&self.editor) {
                    self.is_loading = true;
                    Task::perform(file::save_file(None, content), Message::ProjectSaved)
                } else {
                    Task::none()
                }
            }
            Message::ProjectSaved(result) => {
                self.is_loading = false;

                if let Ok(path) = result {
                    self.file = Some(path);
                }

                Task::none()
            }
            Message::Analyzer(event) => {
                let mut tasks = Vec::new();
                for update in self.analyzer.client().receive(event) {
                    match update {
                        lsp::Update::Document(document) => tasks.push(
                            self.editor
                                .apply_lsp(&mut self.analyzer, document)
                                .map(Message::Editor),
                        ),
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

    pub fn view(&self) -> Element<'_, Message> {
        let panes = self
            .layout
            .view(|_, pane, _| match pane {
                layout::PaneContent::Editor => (
                    self.editor.view(&self.analyzer).map(Message::Editor).into(),
                    self.editor
                        .text()
                        .filename_display_text()
                        .or(Some(String::from("New file"))),
                ),
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
        self.editor.text().theme()
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

        Subscription::batch([self.editor.subscription().map(Message::Editor), escape])
    }
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
