//! The controls for one uniform and the row that adds new ones.

use std::ops::RangeInclusive;

use iced::widget::{column, combo_box, row, text, text_input};
use iced::{Center, Element, FillPortion, Length, Theme};
use num_traits::AsPrimitive;

use super::{ROW_HEIGHT, TYPING_ID, Typing};
use crate::control::{outside, scrub};
use crate::core::uniform::{Error, Type, Uniform, Uniforms};

#[derive(Debug, Clone)]
pub enum Message {
    Set(Type),
    /// Replaces the field of the component at this index with a text input.
    StartTyping(usize),
}

/// A uniform's name and a scrub field for each component of its value, or a text input for
/// the component being typed.
pub fn view<'a>(uniform: &'a Uniform, typing: Option<&'a Typing>) -> Element<'a, super::Message> {
    const AXES: [&str; 4] = ["x", "y", "z", "w"];
    const CHANNELS: [&str; 4] = ["r", "g", "b", "a"];

    let fields = match uniform.value {
        Type::Int(_) | Type::VecInt2(_) | Type::VecInt3(_) | Type::VecInt4(_) => {
            fields::<i32>(uniform, typing, AXES, 0.25, None)
        }
        Type::Float(_) | Type::VecFloat2(_) | Type::VecFloat3(_) | Type::VecFloat4(_) => {
            fields::<f32>(uniform, typing, AXES, 0.01, None)
        }
        Type::Col3(_) | Type::Col4(_) => {
            fields::<f32>(uniform, typing, CHANNELS, 0.005, Some(0.0..=1.0))
        }
    };

    row![text(uniform.name()).width(FillPortion(1)), fields]
        .spacing(4)
        .height(ROW_HEIGHT)
        .align_y(Center)
        .into()
}

/// A scrub field per component of `uniform`'s value, editing it as a `T`, with `typing`'s
/// component as a text input instead. Vectors label each field.
fn fields<'a, T>(
    uniform: &'a Uniform,
    typing: Option<&'a Typing>,
    labels: [&'static str; 4],
    speed: f64,
    range: Option<RangeInclusive<T>>,
) -> Element<'a, super::Message>
where
    T: scrub::Input + 'a,
    f64: AsPrimitive<T>,
{
    let value = uniform.value;
    let (components, count) = value.components();

    row((0..count).map(|index| {
        if let Some(typing) = typing.filter(|typing| typing.component == index) {
            let valid = typing.value(value).is_some();
            let input = text_input("", &typing.text)
                .id(TYPING_ID)
                .on_input(super::Message::TypingChanged)
                .on_submit(super::Message::Submit)
                .size(14)
                .line_height(text::LineHeight::Absolute(ROW_HEIGHT.into()))
                .padding([0, 6])
                .style(move |theme: &Theme, status| {
                    let style = text_input::default(theme, status);
                    if valid {
                        style
                    } else {
                        text_input::Style {
                            border: style.border.color(theme.palette().danger.base.color),
                            ..style
                        }
                    }
                });
            return outside::outside(input, super::Message::Submit).into();
        }

        // The fields keep components the type can hold, so a refusal only keeps the old value.
        let field = scrub::scrub::<T, _>(components[index].as_(), move |component| {
            Message::Set(
                value
                    .with_component(index, component.as_())
                    .unwrap_or(value),
            )
        })
        .on_click(Message::StartTyping(index))
        .speed(speed)
        .height(ROW_HEIGHT);
        let field = if count > 1 {
            field.label(labels[index])
        } else {
            field
        };
        let field: Element<'a, Message> = match range.clone() {
            Some(range) => field.range(range).into(),
            None => field.into(),
        };
        field.map(move |message| super::Message::Uniform {
            name: uniform.name().to_owned(),
            message,
        })
    }))
    .spacing(2)
    .width(FillPortion(3))
    .into()
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
