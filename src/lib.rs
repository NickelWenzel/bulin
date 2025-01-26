mod editor;
mod shader;

use iced::widget::column;
use iced::window;
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
    fn new() -> Self {
        Self {
            editor: editor::Editor::new().0,
            shader: shader::IcedCubes::new(),
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Editor(message) => self.editor.update(message).map(|m| Message::Editor(m)),
            Message::Shader(message) => self.shader.update(message).map(|m| Message::Shader(m)),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        column![
            self.editor.view().map(Message::Editor),
            self.shader.view().map(Message::Shader),
        ]
        .into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        window::frames()
            .map(shader::Message::Tick)
            .map(Message::Shader)
    }

    pub fn theme(&self) -> Theme {
        self.editor.theme()
    }
}

impl Default for Application {
    fn default() -> Self {
        Self::new()
    }
}
