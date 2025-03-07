mod canvasscene;

use crate::pipeline_update::PipelineUpdate;

use canvasscene::CanvasScene;

use iced::widget::shader;
use iced::{Element, Fill, Task};

pub struct Viewer {
    scene: CanvasScene,
}

#[derive(Debug, Clone)]
pub enum Message {
    UpdatePipeline(PipelineUpdate),
}

impl Viewer {
    pub fn new() -> Self {
        Self {
            scene: CanvasScene::new(),
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

impl Default for Viewer {
    fn default() -> Self {
        Self::new()
    }
}
