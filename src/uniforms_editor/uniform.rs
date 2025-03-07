use std::ops::RangeBounds;

use iced::{
    widget::{column, combo_box, horizontal_space, row, text, text_input},
    Element, Length, Task,
};
use iced_aw::number_input;
use num_traits::bounds::Bounded;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub enum Message {
    ChangeValue(Type),
    ChangeName(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Type {
    Int(i32),
    Float(f32),
    VecFloat2((f32, f32)),
    VecFloat3((f32, f32, f32)),
    Col3((f32, f32, f32)),
    VecFloat4((f32, f32, f32, f32)),
    Col4((f32, f32, f32, f32)),
    VecInt2((i32, i32)),
    VecInt3((i32, i32, i32)),
    VecInt4((i32, i32, i32, i32)),
}

impl Type {
    pub const ALL: [Type; 10] = [
        Type::Int(0),
        Type::Float(0.0),
        Type::VecFloat2((0.0, 0.0)),
        Type::VecFloat3((0.0, 0.0, 0.0)),
        Type::Col3((0.5, 0.5, 0.5)),
        Type::VecFloat4((0.0, 0.0, 0.0, 0.0)),
        Type::Col4((0.5, 0.5, 0.5, 1.0)),
        Type::VecInt2((0, 0)),
        Type::VecInt3((0, 0, 0)),
        Type::VecInt4((0, 0, 0, 0)),
    ];

    pub fn to_shader_line(&self) -> String {
        match self {
            Self::Int(_) => String::from("i32"),
            Self::Float(_) => String::from("f32"),
            Self::VecFloat2(_) => String::from("vec2<f32>"),
            Self::VecFloat3(_) => String::from("vec3<f32>"),
            Self::Col3(_) => String::from("vec3<f32>"),
            Self::VecFloat4(_) => String::from("vec4<f32>"),
            Self::Col4(_) => String::from("vec4<f32>"),
            Self::VecInt2(_) => String::from("vec2<i32>"),
            Self::VecInt3(_) => String::from("vec3<i32>"),
            Self::VecInt4(_) => String::from("vec4<i32>"),
        }
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Int(_) => "Int",
            Self::Float(_) => "Float",
            Self::VecFloat2(_) => "VecFloat2",
            Self::VecFloat3(_) => "VecFloat3",
            Self::Col3(_) => "Col3",
            Self::VecFloat4(_) => "VecFloat4",
            Self::Col4(_) => "Col4",
            Self::VecInt2(_) => "VecInt2",
            Self::VecInt3(_) => "VecInt3",
            Self::VecInt4(_) => "VecInt4",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Uniform {
    pub value: Type,
    pub name: String,
}

impl Uniform {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ChangeValue(value) => {
                self.value = value;
                Task::none()
            }
            Message::ChangeName(name) => {
                self.name = name;
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<Message> {
        let name = text(&self.name);
        let value_view = match &self.value {
            Type::Int(value) => {
                number_input(value, -100..100, |v| Message::ChangeValue(Type::Int(v)))
                    .step(1)
                    .into()
            }
            Type::Float(value) => number_input(value, -100.0..100.0, |v| {
                Message::ChangeValue(Type::Float(v))
            })
            .step(1.0)
            .into(),
            Type::VecFloat2(value) => number_input2(value, -100.0..100.0, 1.0, |v| {
                Message::ChangeValue(Type::VecFloat2(v))
            }),
            Type::VecFloat3(value) => number_input3(value, -100.0..100.0, 1.0, |v| {
                Message::ChangeValue(Type::VecFloat3(v))
            }),
            Type::Col3(value) => number_input3(value, 0.0..1.0, 0.05, |v| {
                Message::ChangeValue(Type::Col3(v))
            }),
            Type::VecFloat4(value) => number_input4(value, -100.0..100.0, 1.0, |v| {
                Message::ChangeValue(Type::VecFloat4(v))
            }),
            Type::Col4(value) => number_input4(value, 0.0..1.0, 0.05, |v| {
                Message::ChangeValue(Type::Col4(v))
            }),
            Type::VecInt2(value) => number_input2(value, -100..100, 1, |v| {
                Message::ChangeValue(Type::VecInt2(v))
            }),
            Type::VecInt3(value) => number_input3(value, -100..100, 1, |v| {
                Message::ChangeValue(Type::VecInt3(v))
            }),
            Type::VecInt4(value) => number_input4(value, -100..100, 1, |v| {
                Message::ChangeValue(Type::VecInt4(v))
            }),
        };
        row![name, horizontal_space(), value_view].into()
    }

    pub fn to_shader_line(&self) -> String {
        format!("{}: {}", self.name, self.value.to_shader_line())
    }
}

fn number_input2<'a, T, F>(
    (v0, v1): &'a (T, T),
    bounds: impl RangeBounds<T> + Clone,
    step: T,
    on_change: F,
) -> Element<'a, Message>
where
    F: 'a + Fn((T, T)) -> Message + Copy,
    T: 'a
        + num_traits::Num
        + num_traits::NumAssignOps
        + PartialOrd
        + std::fmt::Display
        + std::str::FromStr
        + Copy
        + Bounded,
{
    row![
        number_input(v0, bounds.clone(), move |v| on_change((v, *v1))).step(step),
        number_input(v1, bounds, move |v| on_change((*v0, v))).step(step),
    ]
    .into()
}

fn number_input3<'a, T, F>(
    (v0, v1, v2): &'a (T, T, T),
    bounds: impl RangeBounds<T> + Clone,
    step: T,
    on_change: F,
) -> Element<'a, Message>
where
    F: 'a + Fn((T, T, T)) -> Message + Copy,
    T: 'a
        + num_traits::Num
        + num_traits::NumAssignOps
        + PartialOrd
        + std::fmt::Display
        + std::str::FromStr
        + Copy
        + Bounded,
{
    row![
        number_input(v0, bounds.clone(), move |v| on_change((
            v,
            v1.clone(),
            v2.clone()
        )))
        .step(step),
        number_input(v1, bounds.clone(), move |v| on_change((
            v0.clone(),
            v,
            v2.clone()
        )))
        .step(step),
        number_input(v2, bounds, move |v| on_change((v0.clone(), v1.clone(), v))).step(step),
    ]
    .into()
}

fn number_input4<'a, T, F>(
    (v0, v1, v2, v3): &'a (T, T, T, T),
    bounds: impl RangeBounds<T> + Clone,
    step: T,
    on_change: F,
) -> Element<'a, Message>
where
    F: 'a + Fn((T, T, T, T)) -> Message + Copy,
    T: 'a
        + num_traits::Num
        + num_traits::NumAssignOps
        + PartialOrd
        + std::fmt::Display
        + std::str::FromStr
        + Copy
        + Bounded,
{
    row![
        number_input(v0, bounds.clone(), move |v| on_change((
            v,
            v1.clone(),
            v2.clone(),
            v3.clone()
        )))
        .step(step),
        number_input(v1, bounds.clone(), move |v| on_change((
            v0.clone(),
            v,
            v2.clone(),
            v3.clone()
        )))
        .step(step),
        number_input(v1, bounds.clone(), move |v| on_change((
            v0.clone(),
            v1.clone(),
            v,
            v3.clone()
        )))
        .step(step),
        number_input(v2, bounds, move |v| on_change((
            v0.clone(),
            v1.clone(),
            v2.clone(),
            v
        )))
        .step(step),
    ]
    .into()
}

impl TryFrom<Candidate> for Uniform {
    type Error = &'static str;

    fn try_from(candidate: Candidate) -> Result<Self, Self::Error> {
        if let Some(value) = candidate.current_type {
            Ok(Self {
                value,
                name: candidate.name,
            })
        } else {
            Err("Value not present.")
        }
    }
}

#[derive(Clone)]
pub struct Candidate {
    types: combo_box::State<Type>,
    pub current_type: Option<Type>,
    pub name: String,
}

#[derive(Debug, Clone)]
pub enum CandidateMessage {
    Selected(Type),
    NameChanged(String),
}

impl Candidate {
    pub fn new() -> Self {
        Self {
            types: combo_box::State::new(Type::ALL.to_vec()),
            current_type: Some(Type::Float(0.0)),
            name: String::default(),
        }
    }

    pub fn view(&self) -> Element<CandidateMessage> {
        let upper_left = row![
            text_input("Name...", &self.name).on_input(CandidateMessage::NameChanged),
            combo_box(
                &self.types,
                "Select type",
                self.current_type.as_ref(),
                CandidateMessage::Selected,
            ),
        ];
        let lower_left = row![];
        column![upper_left, lower_left]
            .height(Length::Shrink)
            .into()
    }

    pub fn update(&mut self, message: CandidateMessage) -> Task<CandidateMessage> {
        match message {
            CandidateMessage::Selected(value) => {
                self.current_type = Some(value);
                Task::none()
            }
            CandidateMessage::NameChanged(name) => {
                self.name = name;
                Task::none()
            }
        }
    }
}
