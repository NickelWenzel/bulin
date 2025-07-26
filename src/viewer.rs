mod canvasscene;

use canvasscene::CanvasScene;

use iced::widget::shader;
use iced::{Element, Fill, Task};

use crate::shader_update::ShaderUpdate;

pub struct Viewer {
    scene: CanvasScene,
}

#[derive(Debug, Clone)]
pub enum Message {
    UpdatePipeline(ShaderUpdate),
}

impl Viewer {
    pub fn new(shader: String) -> Self {
        Self {
            scene: CanvasScene::new(shader),
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::UpdatePipeline(message) => {
                self.scene.update(message);
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        shader(&self.scene).width(Fill).height(Fill).into()
    }
}
