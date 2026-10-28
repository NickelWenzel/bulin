//! The controls for one uniform and the row that adds new ones.

use std::f32::consts::{FRAC_PI_2, PI};

use iced::widget::{
    button, column, combo_box, container, row, rule, space, stack, text, text_input,
};
use iced::{Center, Color, Element, Fill, FillPortion, Length, Point, Theme, border, gradient};
use num_traits::AsPrimitive;

use super::{ROW_HEIGHT, TYPING_ID, Typing};
use crate::control::{outside, pad, popover, scrub};
use crate::core::color;
use crate::core::uniform::{Error, Type, Uniform, Uniforms};

#[derive(Debug, Clone)]
pub enum Message {
    Set(Type),
    /// Replaces the field of the component at this index with a text input.
    StartTyping(usize),
    /// Opens the value's editor in a popover.
    Open,
    /// Moves a vector to this point of its pad, in `[0, 1]²`.
    Pad(Point),
    /// Sets a color's saturation and value to this point's `x` and `y`, keeping its hue.
    Sv(Point),
    /// Sets a color's hue, keeping its saturation and value.
    Hue(f32),
    Alpha(f32),
    HexChanged(String),
    /// Applies the typed hex code when the color can hold it.
    HexSubmitted,
}

/// The open popover of a uniform, holding what its value can't give back.
#[derive(Debug, Clone, PartialEq)]
pub enum Editor {
    /// An XY pad spanning `[-extent, extent]²`, chosen when it opens so it doesn't move along.
    Pad { extent: f32 },
    /// An HSV picker. The hue and saturation are kept for when the value can't tell them, as
    /// for black and greys, and the hex code being typed, if any, replaces the derived one.
    Color {
        hue: f32,
        saturation: f32,
        hex_text: Option<String>,
    },
}

const AXES: [&str; 4] = ["x", "y", "z", "w"];

const CHANNELS: [&str; 4] = ["r", "g", "b", "a"];

/// The side of the pad's square, in logical pixels.
const PAD_SIZE: f32 = 160.0;

/// The thickness of a color picker's hue and alpha strips, in logical pixels.
const STRIP: f32 = 12.0;

/// A uniform's name and a scrub field for each component of its value, or a text input for
/// the component being typed. A vec2 has a button opening its `editor` next to it, and a color
/// shows a swatch and its hex code that open it.
pub fn view<'a>(
    uniform: &'a Uniform,
    typing: Option<&'a Typing>,
    editor: Option<&'a Editor>,
) -> Element<'a, super::Message> {
    let fields = match uniform.value {
        Type::Int(_) | Type::VecInt2(_) | Type::VecInt3(_) | Type::VecInt4(_) => {
            fields::<i32>(uniform, typing, 0.25)
        }
        Type::Float(_) | Type::VecFloat3(_) | Type::VecFloat4(_) => {
            fields::<f32>(uniform, typing, 0.01)
        }
        Type::VecFloat2(value) => {
            let anchor = button(text("xy").size(14).center().height(Fill))
                .on_press(super::Message::Uniform {
                    name: uniform.name().to_owned(),
                    message: Message::Open,
                })
                .style(button::text)
                .padding([0, 4])
                .height(ROW_HEIGHT);
            let content = match editor {
                Some(&Editor::Pad { extent }) => Some(xy_pad(uniform, value, extent)),
                _ => None,
            };

            row![
                fields::<f32>(uniform, typing, 0.01),
                popover::popover(anchor, content, super::Message::Close),
            ]
            .spacing(2)
            .width(FillPortion(3))
            .into()
        }
        Type::Col3(_) | Type::Col4(_) => {
            let ([r, g, b, a], count) = uniform.value.components();
            let rgb = (r as f32, g as f32, b as f32);
            let alpha = (count == 4).then_some(a as f32);
            let fill = Color::from_rgba(rgb.0, rgb.1, rgb.2, alpha.unwrap_or(1.0));
            let size = ROW_HEIGHT - 6.0;
            let swatch = container(space())
                .width(size)
                .height(size)
                .style(move |theme: &Theme| container::Style {
                    background: Some(fill.into()),
                    border: border::rounded(2)
                        .color(theme.palette().background.strong.color)
                        .width(1),
                    ..container::Style::default()
                });
            let swatch: Element<'a, super::Message> = if alpha.is_some() {
                stack![backdrop(), swatch].width(size).height(size).into()
            } else {
                swatch.into()
            };
            let anchor = button(
                row![swatch, text(color::hex(rgb, alpha)).size(14)]
                    .spacing(6)
                    .height(Fill)
                    .align_y(Center),
            )
            .on_press(super::Message::Uniform {
                name: uniform.name().to_owned(),
                message: Message::Open,
            })
            .style(button::text)
            .padding([0, 4])
            .width(FillPortion(3))
            .height(ROW_HEIGHT);
            let content = match editor {
                Some(Editor::Color {
                    hue,
                    saturation,
                    hex_text,
                }) => Some(picker(
                    uniform,
                    rgb,
                    alpha,
                    *hue,
                    *saturation,
                    hex_text.as_deref(),
                )),
                _ => None,
            };

            popover::popover(anchor, content, super::Message::Close).into()
        }
    };

    row![text(uniform.name()).width(FillPortion(1)), fields]
        .spacing(4)
        .height(ROW_HEIGHT)
        .align_y(Center)
        .into()
}

/// The popover of a vec2: a pad over axes spanning `[-extent, extent]²`, the extent, and the
/// components' fields.
fn xy_pad<'a>(
    uniform: &'a Uniform,
    (x, y): (f32, f32),
    extent: f32,
) -> Element<'a, super::Message> {
    let value = uniform.value;
    let axes = stack![
        container(rule::horizontal(1)).center_y(Fill),
        container(rule::vertical(1)).center_x(Fill),
        pad::pad(
            Point::new((x / extent + 1.0) / 2.0, (y / extent + 1.0) / 2.0),
            Message::Pad,
        ),
    ]
    .width(PAD_SIZE)
    .height(PAD_SIZE);
    let fields = row((0..2).map(|index| {
        scrub::scrub([x, y][index], move |component: f32| {
            Message::Set(
                value
                    .with_component(index, component.into())
                    .unwrap_or(value),
            )
        })
        .label(AXES[index])
        .speed(0.01)
        .height(ROW_HEIGHT)
        .into()
    }))
    .spacing(2);

    let content: Element<'a, Message> = column![
        axes,
        text(format!("extent {extent}"))
            .size(12)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.palette().background.base.text.scale_alpha(0.5)),
            }),
        fields,
    ]
    .spacing(4)
    .padding(8)
    .width(PAD_SIZE + 16.0)
    .into();
    content.map(move |message| super::Message::Uniform {
        name: uniform.name().to_owned(),
        message,
    })
}

/// The popover of a color: a saturation-value square and a hue strip for the color `rgb` of
/// `hue` and `saturation`, an alpha strip when it has an `alpha`, its hex code, or the
/// `hex_text` being typed, and its channels' fields.
fn picker<'a>(
    uniform: &'a Uniform,
    rgb: (f32, f32, f32),
    alpha: Option<f32>,
    hue: f32,
    saturation: f32,
    hex_text: Option<&'a str>,
) -> Element<'a, super::Message> {
    let value = uniform.value;
    let gradient = |gradient: gradient::Linear| {
        move |_theme: &Theme| container::Style {
            background: Some(gradient.into()),
            ..container::Style::default()
        }
    };
    let pure = |hue: f32| {
        let (r, g, b) = color::Hsv {
            hue,
            saturation: 1.0,
            value: 1.0,
        }
        .to_rgb();
        Color::from_rgb(r, g, b)
    };
    let (r, g, b) = rgb;

    // An angle of π/2 runs left to right, and π top to bottom.
    let sv = stack![
        container(space()).width(Fill).height(Fill).style(gradient(
            gradient::Linear::new(FRAC_PI_2)
                .add_stop(0.0, Color::WHITE)
                .add_stop(1.0, pure(hue))
        )),
        container(space()).width(Fill).height(Fill).style(gradient(
            gradient::Linear::new(PI)
                .add_stop(0.0, Color::TRANSPARENT)
                .add_stop(1.0, Color::BLACK)
        )),
        pad::pad(
            Point::new(saturation, color::Hsv::from_rgb(rgb).value),
            Message::Sv,
        ),
    ]
    .width(PAD_SIZE)
    .height(PAD_SIZE);
    let hues = (0..=6).fold(gradient::Linear::new(FRAC_PI_2), |hues, sixth| {
        let offset = sixth as f32 / 6.0;
        hues.add_stop(offset, pure(offset))
    });
    let hue_strip = stack![
        container(space())
            .width(Fill)
            .height(Fill)
            .style(gradient(hues)),
        pad::pad(Point::new(hue, 0.5), |point| Message::Hue(point.x)),
    ]
    .width(PAD_SIZE)
    .height(STRIP);
    let alpha_strip = alpha.map(|alpha| {
        stack![
            backdrop(),
            container(space()).width(Fill).height(Fill).style(gradient(
                gradient::Linear::new(FRAC_PI_2)
                    .add_stop(0.0, Color::from_rgba(r, g, b, 0.0))
                    .add_stop(1.0, Color::from_rgb(r, g, b))
            )),
            pad::pad(Point::new(alpha, 0.5), |point| Message::Alpha(point.x)),
        ]
        .width(PAD_SIZE)
        .height(STRIP)
    });

    let valid = hex_text.is_none_or(|text| {
        color::parse_hex(text).is_some_and(|(_, typed)| typed.is_none() || alpha.is_some())
    });
    let hex = text_input(
        "",
        hex_text.map_or_else(|| color::hex(rgb, alpha), str::to_owned),
    )
    .on_input(Message::HexChanged)
    .on_submit(Message::HexSubmitted)
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

    let (components, count) = value.components();
    let fields = row((0..count).map(|index| {
        scrub::scrub(components[index] as f32, move |component: f32| {
            Message::Set(
                value
                    .with_component(index, component.into())
                    .unwrap_or(value),
            )
        })
        .label(CHANNELS[index])
        .speed(0.005)
        .range(0.0..=1.0)
        .height(ROW_HEIGHT)
        .into()
    }))
    .spacing(2);

    let content: Element<'a, Message> = column![sv, hue_strip]
        .push(alpha_strip)
        .push(hex)
        .push(fields)
        .spacing(6)
        .padding(8)
        .width(PAD_SIZE + 16.0)
        .into();
    content.map(move |message| super::Message::Uniform {
        name: uniform.name().to_owned(),
        message,
    })
}

/// Two tones, light above dark, for a translucent color to show its alpha against.
fn backdrop<'a, Message: 'a>() -> Element<'a, Message> {
    let tone = |tone: Color| {
        container(space())
            .width(Fill)
            .height(Fill)
            .style(move |_theme: &Theme| container::Style {
                background: Some(tone.into()),
                ..container::Style::default()
            })
    };

    column![tone(Color::WHITE), tone(Color::from_rgb(0.4, 0.4, 0.4))].into()
}

/// A scrub field per component of `uniform`'s value, editing it as a `T`, with `typing`'s
/// component as a text input instead. Vectors label each field.
fn fields<'a, T>(
    uniform: &'a Uniform,
    typing: Option<&'a Typing>,
    speed: f64,
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
        let field: Element<'a, Message> = if count > 1 {
            field.label(AXES[index]).into()
        } else {
            field.into()
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
