mod editor;
mod shader;

use iced::time::Instant;
use iced::widget::column;
use iced::window;
use iced::{Element, Subscription};

pub struct Application {
    editor: editor::Editor,
    shader: shader::IcedCubes,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick(Instant),
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

    pub fn update(&mut self, _message: Message) {}
    pub fn view(&self) -> Element<'_, Message> {
        column![
            self.editor.view().map(Message::Editor),
            self.shader.view().map(Message::Shader),
        ]
        .into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        window::frames().map(Message::Tick)
    }
}

impl Default for Application {
    fn default() -> Self {
        Self::new()
    }
}
