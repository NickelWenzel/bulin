use std::convert::identity;

use iced::{
    Alignment, Color, Element, Length, Task, Theme, border,
    widget::{column, combo_box, container, row, slider, space, stack, text, text_input},
};

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

    pub fn to_shader_line(self) -> String {
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
            Type::Int(value) => input_slider(value, (ir, is)).map(Type::Int),
            Type::Float(value) => input_slider(value, (fr, fs)).map(Type::Float),
            Type::VecFloat2(value) => input_slider2(value, (fr, fs)).map(Type::VecFloat2),
            Type::VecFloat3(value) => input_slider3(value, (fr, fs)).map(Type::VecFloat3),
            Type::Col3(value) => input_slider3(value, (0.0..=1.0, 0.05)).map(Type::Col3),
            Type::VecFloat4(value) => input_slider4(value, (fr, fs)).map(Type::VecFloat4),
            Type::Col4(value) => input_slider4(value, (0.0..=1.0, 0.05)).map(Type::Col4),
            Type::VecInt2(value) => input_slider2(value, (ir, is)).map(Type::VecInt2),
            Type::VecInt3(value) => input_slider3(value, (ir, is)).map(Type::VecInt3),
            Type::VecInt4(value) => input_slider4(value, (ir, is)).map(Type::VecInt4),
        }
        .map(Message::ChangeValue);

        row![name, space::horizontal(), value_view].into()
    }

    pub fn to_shader_line(&self) -> String {
        format!("{}: {}", self.name, self.value.to_shader_line())
    }
}

fn input_slider<'a, T: Input>(
    value: T,
    (range, step): (std::ops::RangeInclusive<T>, T),
) -> Element<'a, T> {
    value_slider((range, step), value, identity)
}

fn input_slider2<'a, T: Input>(
    v: (T, T),
    (range, step): (std::ops::RangeInclusive<T>, T),
) -> Element<'a, (T, T)> {
    row![
        value_slider((range.clone(), step), v.0, move |v0| (v0, v.1)),
        value_slider((range, step), v.1, move |v1| (v.0, v1)),
    ]
    .into()
}

fn input_slider3<'a, T: Input>(
    v: (T, T, T),
    (range, step): (std::ops::RangeInclusive<T>, T),
) -> Element<'a, (T, T, T)> {
    row![
        value_slider((range.clone(), step), v.0, move |v0| (v0, v.1, v.2)),
        value_slider((range.clone(), step), v.1, move |v1| (v.0, v1, v.2)),
        value_slider((range, step), v.2, move |v2| (v.0, v.1, v2)),
    ]
    .into()
}

fn input_slider4<'a, T: Input>(
    v: (T, T, T, T),
    (range, step): (std::ops::RangeInclusive<T>, T),
) -> Element<'a, (T, T, T, T)> {
    row![
        value_slider((range.clone(), step), v.0, move |v0| (v0, v.1, v.2, v.3)),
        value_slider((range.clone(), step), v.1, move |v1| (v.0, v1, v.2, v.3)),
        value_slider((range.clone(), step), v.2, move |v2| (v.0, v.1, v2, v.3)),
        value_slider((range, step), v.3, move |v3| (v.0, v.1, v.2, v3)),
    ]
    .into()
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

fn value_slider<'a, T, Message, Renderer>(
    (range, step): (std::ops::RangeInclusive<T>, T),
    current: T,
    set: impl Fn(T) -> Message + 'a,
) -> Element<'a, Message, Theme, Renderer>
where
    T: Input,
    Message: Clone + 'a,
    Renderer: iced::advanced::text::Renderer + 'a,
{
    stack![
        container(
            slider(range, current, set)
                .step(step)
                .width(Length::Fill)
                .height(24)
                .style(slider_style)
        )
        .style(|theme| container::Style::default()
            .background(theme.palette().background.weak.color)
            .border(border::rounded(2))),
        row![space::horizontal(), text(format!("{current}")).size(14)]
            .padding([0, 10])
            .height(Length::Fill)
            .align_y(Alignment::Center),
    ]
    .into()
}

fn slider_style(theme: &Theme, status: slider::Status) -> slider::Style {
    let palette = theme.palette();

    slider::Style {
        rail: slider::Rail {
            backgrounds: (
                match status {
                    slider::Status::Active | slider::Status::Dragged => {
                        palette.background.strongest.color
                    }
                    slider::Status::Hovered => palette.background.stronger.color,
                }
                .into(),
                Color::TRANSPARENT.into(),
            ),
            width: 24.0,
            border: border::rounded(2),
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 0.0 },
            background: Color::TRANSPARENT.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    }
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
