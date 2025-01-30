mod editor;
mod viewer;

use iced::widget::row;
use iced::{Element, Task, Theme};

pub type FragmentShader = String;

pub struct Application {
    editor: editor::Editor,
    viewer: viewer::Viewer,
}

#[derive(Debug, Clone)]
pub enum Message {
    Editor(editor::Message),
    Viewer(viewer::Message),
}

impl Application {
    pub fn new() -> (Self, Task<Message>) {
        let (editor, editor_task) = editor::Editor::new();
        (
            Self {
                editor,
                viewer: viewer::Viewer::new(),
            },
            editor_task.map(|m| Message::Editor(m)),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Editor(message) => match message {
                editor::Message::UpdatePipeline(shader) => self
                    .viewer
                    .update(viewer::Message::UpdatePipeline(shader)).map(|m| Message::Viewer(m)),
                _ => self.editor.update(message).map(|m| Message::Editor(m)),
            },
            Message::Viewer(message) => self.viewer.update(message).map(|m| Message::Viewer(m)),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        row![
            self.editor.view().map(Message::Editor),
            self.viewer.view().map(Message::Viewer),
        ]
        .into()
    }

    pub fn theme(&self) -> Theme {
        self.editor.theme()
    }
}
