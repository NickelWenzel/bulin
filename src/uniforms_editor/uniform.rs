use iced::{Element, Task};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub enum Message {
    ChangeValue(Type),
    ChangeName(String),
}

#[derive(Serialize, Deserialize)]
#[derive(Debug, Clone)]
pub enum Type {
    Int,
    Float,
    VecFloat2,
    VecFloat3,
    Col3,
    VecFloat4,
    Col4,
    VecInt2,
    VecInt3,
    VecInt4,
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub struct Uniform {
    pub value: Type,
    pub name: String,
}

impl Uniform {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        return Task::none();
    }

    pub fn view(&self) -> Element<Message> {
        Element::new(iced::widget::column![])
    }
}
