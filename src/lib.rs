mod editor;
mod layout;
mod menu;
mod pipeline_update;
mod text_editor;
mod uniforms_editor;
mod util;
mod viewer;

use pipeline_update::PipelineUpdate;

use iced::widget::{column, container};
use iced::{Element, Length, Subscription, Task, Theme};
use util::Error;

use std::path::PathBuf;
use std::sync::Arc;

pub struct Application {
    editor: editor::Editor,
    viewer: viewer::Viewer,
    layout: layout::Layout,
    file: Option<PathBuf>,
    is_loading: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Editor(editor::Message),
    Viewer(viewer::Message),
    Layout(layout::Message),
    Menu(menu::Message),
    OpenProject,
    ProjectOpened(Result<(PathBuf, Arc<String>), Error>),
    SaveProject,
    ProjectSaved(Result<PathBuf, Error>),
}

impl Application {
    pub fn new() -> (Self, Task<Message>) {
        let (editor, editor_task) = editor::Editor::new();
        (
            Self {
                editor,
                viewer: viewer::Viewer::new(),
                layout: layout::Layout::new(),
                file: None,
                is_loading: false,
            },
            editor_task.map(Message::Editor),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Editor(message) => match message {
                editor::Message::UpdatePipeline(update) => self
                    .viewer
                    .update(viewer::Message::UpdatePipeline(update))
                    .map(Message::Viewer),
                _ => self.editor.update(message).map(Message::Editor),
            },
            Message::Viewer(message) => self.viewer.update(message).map(Message::Viewer),
            Message::Layout(message) => self.layout.update(message).map(Message::Layout),
            Message::Menu(message) => match message {
                menu::Message::OpenProject => Task::done(Message::OpenProject),
                menu::Message::SaveProject => Task::done(Message::SaveProject),
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
            Message::ProjectOpened(result) => {
                self.is_loading = false;

                if let Ok((path, contents)) = result {
                    if let Ok(editor) = serde_json::from_str(&contents) {
                        self.file = Some(path);
                        self.editor = editor;
                        Task::done(PipelineUpdate::Shader(self.editor.text().content()))
                            .map(viewer::Message::UpdatePipeline)
                            .map(Message::Viewer)
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
            Message::ProjectSaved(result) => {
                self.is_loading = false;

                if let Ok(path) = result {
                    self.file = Some(path);
                }

                Task::none()
            }
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

        column!(
            menu::view().map(Message::Menu),
            container(panes).width(Length::Fill).height(Length::Fill),
        )
        .into()
    }

    pub fn theme(&self) -> Theme {
        self.editor.text().theme()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        self.editor.subscription().map(Message::Editor)
    }
}
