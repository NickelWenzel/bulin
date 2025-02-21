mod canvasscene;

use crate::pipeline_update::FragmentShader;
use crate::pipeline_update::PipelineUpdate;

use canvasscene::CanvasScene;

use iced::time::Instant;
use iced::widget::shader;
use iced::{Element, Fill, Task};

use std::sync::Arc;

pub struct Viewer {
    scene: CanvasScene,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick(Instant),
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
            Message::UpdatePipeline(PipelineUpdate::Shader(shader)) => {
                self.scene.update(shader);
                Task::none()
            }
            _ => Task::none(),
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
