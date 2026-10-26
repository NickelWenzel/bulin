//! The controls for one uniform and the row that adds new ones.

use std::convert::identity;

use iced::{
    Alignment, Color, Element, Length, Theme, border,
    widget::{column, combo_box, container, row, slider, space, stack, text, text_input},
};

use crate::core::uniform::{Error, Type, Uniform, Uniforms};

/// A uniform's name and sliders for its value.
pub fn view(uniform: &Uniform) -> Element<'_, Type> {
    let fr = -100.0..=100.0;
    let fs = 0.1;

    let ir = -100..=100;
    let is = 1;

    let value = match uniform.value {
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
    };

    row![text(uniform.name()), space::horizontal(), value].into()
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

/// The row for naming and typing a new uniform.
pub struct Candidate {
    types: combo_box::State<Type>,
    value: Type,
    name: String,
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
            value: Type::Float(0.0),
            name: String::default(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Adds the candidate to `uniforms` and clears its name for the next one.
    pub fn submit(&mut self, uniforms: &mut Uniforms) -> Result<(), Error> {
        uniforms.add(&self.name, self.value)?;
        self.name.clear();
        Ok(())
    }

    /// The input row, with `check`'s complaint about the typed name below it.
    pub fn view<'a>(&'a self, check: &Result<(), Error>) -> Element<'a, CandidateMessage> {
        let input = row![
            text_input("Name...", &self.name).on_input(CandidateMessage::NameChanged),
            combo_box(
                &self.types,
                "Select type",
                Some(&self.value),
                CandidateMessage::Selected,
            ),
        ];
        let problem = match check {
            Err(error) if !self.name.is_empty() => Some(text(error.to_string()).size(12)),
            _ => None,
        };

        column![input].push(problem).height(Length::Shrink).into()
    }

    pub fn update(&mut self, message: CandidateMessage) {
        match message {
            CandidateMessage::Selected(value) => self.value = value,
            CandidateMessage::NameChanged(name) => self.name = name,
        }
    }
}

impl Default for Candidate {
    fn default() -> Self {
        Self::new()
    }
}
