mod editor;
mod layout;
mod menu;
mod shader_update;
mod text_editor;
mod uniforms_editor;
mod util;
mod viewer;

use iced::keyboard::key;
use iced::widget::{button, center, column, container, mouse_area, opaque, stack, text};
use iced::{keyboard, Color, Element, Event, Font, Length, Subscription, Task, Theme};
use util::Error;

use std::sync::Arc;

use crate::util::FileName;

pub struct Application {
    editor: editor::Editor,
    viewer: viewer::Viewer,
    layout: layout::Layout,
    file: Option<FileName>,
    is_loading: bool,
    show_menu: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Editor(editor::Message),
    Viewer(viewer::Message),
    Layout(layout::Message),
    ShowMenu,
    CloseMenu,
    Menu(menu::Message),
    OpenProject,
    NewProject,
    ProjectOpened(Result<(FileName, Arc<String>), Error>),
    SaveProject,
    SaveProjectAs,
    ProjectSaved(Result<FileName, Error>),
    Event(Event),
}

impl Application {
    pub fn new(shader: String) -> Self {
        Self {
            editor: editor::Editor::new(&shader),
            viewer: viewer::Viewer::new(shader),
            layout: layout::Layout::new(),
            file: None,
            is_loading: false,
            show_menu: false,
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Editor(message) => match message {
                editor::Message::UpdatePipeline(update) => {
                    Task::done(Message::Viewer(viewer::Message::UpdatePipeline(update)))
                }
                _ => self.editor.update(message).map(Message::Editor),
            },
            Message::Viewer(message) => self.viewer.update(message).map(Message::Viewer),
            Message::Layout(message) => self.layout.update(message).map(Message::Layout),
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

                    Task::perform(util::open_file(), Message::ProjectOpened)
                }
            }
            Message::NewProject => {
                if !self.is_loading {
                    self.file = None;
                    self.editor = editor::Editor::new("");
                }

                self.editor
                    .update(editor::Message::ProjectOpened)
                    .map(Message::Editor)
            }
            Message::ProjectOpened(result) => {
                self.is_loading = false;

                if let Ok((path, contents)) = result {
                    if let Ok(editor) = serde_json::from_str(&contents) {
                        self.file = Some(path);
                        self.editor = editor;
                        self.editor
                            .update(editor::Message::ProjectOpened)
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
                        util::save_file(self.file.clone(), content),
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
                    Task::perform(util::save_file(None, content), Message::ProjectSaved)
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
            Message::Event(event) => match event {
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(key::Named::Escape),
                    ..
                }) => Task::done(Message::CloseMenu),
                _ => Task::none(),
            },
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let panes = self
            .layout
            .view(|_, pane, _| match pane {
                layout::PaneContent::Editor => (
                    self.editor.view().map(Message::Editor).into(),
                    self.editor
                        .text()
                        .filename_display_text()
                        .or(Some(String::from("New file"))),
                ),
                layout::PaneContent::Viewer => {
                    (self.viewer.view().map(Message::Viewer).into(), None)
                }
            })
            .on_click(|e| Message::Layout(layout::Message::Clicked(e)))
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
        self.editor.subscription().map(Message::Editor)
    }
}

impl Default for Application {
    fn default() -> Self {
        Self::new(include_str!("viewer/canvasscene/shaders/empty_frag.wgsl").to_string())
    }
}

fn menu_icon<'a>() -> Element<'a, Message> {
    const MENU_FONT: Font = Font::with_name("menu");

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
