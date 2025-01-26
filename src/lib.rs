mod editor;
mod shader;

use iced::widget::row;
use iced::{Element, Subscription, Task, Theme};

pub struct Application {
    editor: editor::Editor,
    shader: shader::IcedCubes,
}

#[derive(Debug, Clone)]
pub enum Message {
    Editor(editor::Message),
    Shader(shader::Message),
}

impl Application {
    pub fn new() -> (Self, Task<Message>) {
        let (editor, editor_task) = editor::Editor::new();
        (
            Self {
                editor,
                shader: shader::IcedCubes::new(),
            },
            editor_task.map(|m| Message::Editor(m)),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Editor(message) => self.editor.update(message).map(|m| Message::Editor(m)),
            Message::Shader(message) => self.shader.update(message).map(|m| Message::Shader(m)),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        row![
            self.editor.view().map(Message::Editor),
            self.shader.view().map(Message::Shader),
        ]
        .into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        self.shader.subscription().map(|m| Message::Shader(m))
    }

    pub fn theme(&self) -> Theme {
        self.editor.theme()
    }
}
