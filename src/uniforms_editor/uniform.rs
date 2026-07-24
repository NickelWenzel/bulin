use std::convert::identity;

use iced::{
    Element, Length, Task,
    widget::{column, combo_box, row, space, text, text_input},
};

use iced_palace::widget::labeled_slider;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub enum Message {
    ChangeValue(Type),
    ChangeName(String),
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
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

    pub fn view(&'_ self) -> Element<'_, Message> {
        let fr = -100.0..=100.0;
        let fs = 0.1;

        let ir = -100..=100;
        let is = 1;

        let name = text(&self.name);
        let value_view = match self.value {
            Type::Int(value) => input_slider(value, ir, is).map(Type::Int),
            Type::Float(value) => input_slider(value, fr, fs).map(Type::Float),
            Type::VecFloat2(value) => input_slider2(value, fr, fs).map(Type::VecFloat2),
            Type::VecFloat3(value) => input_slider3(value, fr, fs).map(Type::VecFloat3),
            Type::Col3(value) => input_slider3(value, 0.0..=1.0, 0.05).map(Type::Col3),
            Type::VecFloat4(value) => input_slider4(value, fr, fs).map(Type::VecFloat4),
            Type::Col4(value) => input_slider4(value, 0.0..=1.0, 0.05).map(Type::Col4),
            Type::VecInt2(value) => input_slider2(value, ir, is).map(Type::VecInt2),
            Type::VecInt3(value) => input_slider3(value, ir, is).map(Type::VecInt3),
            Type::VecInt4(value) => input_slider4(value, ir, is).map(Type::VecInt4),
        }
        .map(Message::ChangeValue);

        row![name, space::horizontal(), value_view].into()
    }

    pub fn to_shader_line(&self) -> String {
        format!("{}: {}", self.name, self.value.to_shader_line())
    }
}

trait Input:
    Copy
    + PartialOrd
    + From<u8>
    + num_traits::FromPrimitive
    + num_traits::AsPrimitive<f64>
    + std::fmt::Display
    + 'static
{
}

impl<T> Input for T where
    T: Copy
        + PartialOrd
        + From<u8>
        + num_traits::FromPrimitive
        + num_traits::AsPrimitive<f64>
        + std::fmt::Display
        + 'static
{
}

fn input_slider<'a, T: Input>(
    value: T,
    range: std::ops::RangeInclusive<T>,
    step: T,
) -> Element<'a, T> {
    labeled_slider("", (range, step), value, identity, |value| {
        format!("{value:.2}")
    })
    .into()
}

fn input_slider2<'a, T: Input>(
    v: (T, T),
    range: std::ops::RangeInclusive<T>,
    step: T,
) -> Element<'a, (T, T)> {
    let input_slider = |value: T, set: fn((T, T), T) -> (T, T)| {
        labeled_slider(
            "",
            (range.clone(), step),
            value,
            move |value| set(v, value),
            |value| format!("{value:.2}",),
        )
    };

    row![
        input_slider(v.0, |(_, v1), v0| (v0, v1)),
        input_slider(v.1, |(v0, _), v1| (v0, v1)),
    ]
    .into()
}

fn input_slider3<'a, T: Input>(
    v: (T, T, T),
    range: std::ops::RangeInclusive<T>,
    step: T,
) -> Element<'a, (T, T, T)> {
    let input_slider = |value: T, set: fn((T, T, T), T) -> (T, T, T)| {
        labeled_slider(
            "",
            (range.clone(), step),
            value,
            move |value| set(v, value),
            |value| format!("{value:.2}",),
        )
    };

    row![
        input_slider(v.0, |(_, v1, v2), v0| (v0, v1, v2)),
        input_slider(v.1, |(v0, _, v2), v1| (v0, v1, v2)),
        input_slider(v.2, |(v0, v1, _), v2| (v0, v1, v2)),
    ]
    .into()
}

fn input_slider4<'a, T: Input>(
    v: (T, T, T, T),
    range: std::ops::RangeInclusive<T>,
    step: T,
) -> Element<'a, (T, T, T, T)> {
    let input_slider = |value: T, set: fn((T, T, T, T), T) -> (T, T, T, T)| {
        labeled_slider(
            "",
            (range.clone(), step),
            value,
            move |value| set(v, value),
            |value| format!("{value:.2}",),
        )
    };

    row![
        input_slider(v.0, |(_, v1, v2, v3), v0| (v0, v1, v2, v3)),
        input_slider(v.1, |(v0, _, v2, v3), v1| (v0, v1, v2, v3)),
        input_slider(v.2, |(v0, v1, _, v3), v2| (v0, v1, v2, v3)),
        input_slider(v.3, |(v0, v1, v2, _), v3| (v0, v1, v2, v3)),
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

    pub fn view<'a>(&'a self) -> Element<'a, CandidateMessage> {
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

impl Default for Candidate {
    fn default() -> Self {
        Self::new()
    }
}
