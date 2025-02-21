mod time;
pub mod uniform;

use crate::pipeline_update::*;
use uniform::Uniform;

use iced::{widget::container, Element, Task};
use serde::{Deserialize, Serialize};
use std::vec::Vec;

#[derive(Debug, Clone)]
pub enum Message {
    Update(PipelineUpdate),
}

#[derive(Serialize, Deserialize)]
pub struct UniformsEditor {
    uniforms: Vec<Uniform>,
    current_add_candidate: Uniform,
    time: Option<time::Time>,
}

impl UniformsEditor {
    pub fn new() -> Self {
        Self {
            uniforms: Vec::new(),
            current_add_candidate: Uniform {
                value: uniform::Type::Int,
                name: String::new(),
            },
            time: Option::None,
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        return Task::none();
    }

    pub fn view(&self) -> Element<Message> {
        container(iced::widget::column![]).into()
    }
}
