use iced::{Element, Task};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone)]
pub enum Message {
    Pause,
    Resume,
    Reset,
}

#[derive(Serialize, Deserialize)]
pub struct Time {
    #[serde(default = "Instant::now", skip)]
    start: Instant,
}

impl Time {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        return Task::none();
    }

    pub fn view(&self) -> Element<Message> {
        Element::new(iced::widget::column![])
    }
}
