//! The uniforms editor: the user's uniforms and the clock behind `time`.

mod clock;
mod uniform;

use iced::keyboard::{self, key};
use iced::widget::{self, button, column, operation, row, space, text};
use iced::{Element, Fill, Subscription, Task};

use crate::action::Action;
use crate::core::uniform::{Error, Type, join, split};
use crate::core::{Clock, Uniforms, color};

/// The height of a uniform's row and its controls.
const ROW_HEIGHT: f32 = 24.0;

/// The text input of the component being typed. Only one exists at a time.
const TYPING_ID: widget::Id = widget::Id::new("typing");

#[derive(Debug, Clone)]
pub enum Message {
    Clock(clock::Message),
    Uniform {
        name: String,
        message: uniform::Message,
    },
    TypingChanged(String),
    /// Applies the typed text when the component can hold it.
    Submit,
    CancelTyping,
    /// Closes the open popover.
    Close,
    Candidate(uniform::CandidateMessage),
    Add,
    Remove(String),
    AddTime,
    RemoveTime,
}

pub enum Instruction {
    /// Uniform values or the clock changed; the layout is the same.
    Values,
    /// Uniforms or time were added or removed.
    Layout,
}

pub struct State {
    uniforms: Uniforms,
    clock: Option<clock::State>,
    candidate: uniform::Candidate,
    /// Whether the candidate's name can be added to the uniforms as they are now.
    candidate_check: Result<(), Error>,
    typing: Option<Typing>,
    /// The uniform whose popover is open, and its editor's state. Only one is open at a time.
    editor: Option<(String, uniform::Editor)>,
}

/// A component whose value is being typed as text.
pub struct Typing {
    name: String,
    component: usize,
    text: String,
}

impl State {
    pub fn new(uniforms: Uniforms, clock: Option<Clock>) -> Self {
        let candidate = uniform::Candidate::new();
        Self {
            candidate_check: uniforms.check(candidate.name()),
            uniforms,
            clock: clock.map(clock::State::from),
            candidate,
            typing: None,
            editor: None,
        }
    }

    pub fn update(&mut self, message: Message) -> Action<Instruction, Message> {
        let action = match message {
            Message::Clock(message) => match self.clock.as_mut() {
                Some(clock) => {
                    clock.update(message);
                    Action::instruction(Instruction::Values)
                }
                None => Action::none(),
            },
            Message::Uniform {
                name,
                message: uniform::Message::Set(value),
            } => {
                self.typing = None;
                if !self.uniforms.set(&name, value) {
                    return Action::none();
                }
                match &mut self.editor {
                    Some((
                        open,
                        uniform::Editor::Color {
                            hue,
                            saturation,
                            hex_text,
                        },
                    )) if *open == name => {
                        follow(value, hue, saturation);
                        *hex_text = None;
                    }
                    // A zero vector has no direction to give, so the chosen one stays.
                    Some((open, uniform::Editor::Direction { direction })) if *open == name => {
                        if let Type::VecFloat3(vector) = value
                            && let Some((_, new)) = split(vector)
                        {
                            *direction = new;
                        }
                    }
                    _ => {}
                }
                Action::instruction(Instruction::Values)
            }
            Message::Uniform {
                name,
                message: uniform::Message::StartTyping(component),
            } => {
                let Some(uniform) = self.uniforms.iter().find(|uniform| uniform.name() == name)
                else {
                    return Action::none();
                };
                // The fields show rounded values, so the text starts from the stored one.
                let value = uniform.value.components().0[component];
                let text = match uniform.value {
                    Type::Int(_) | Type::VecInt2(_) | Type::VecInt3(_) | Type::VecInt4(_) => {
                        value.to_string()
                    }
                    _ => (value as f32).to_string(),
                };
                self.typing = Some(Typing {
                    name,
                    component,
                    text,
                });
                Action::task(Task::batch([
                    operation::focus(TYPING_ID),
                    operation::select_all(TYPING_ID),
                ]))
            }
            Message::Uniform {
                name,
                message: uniform::Message::Open,
            } => {
                self.typing = None;
                let Some(uniform) = self.uniforms.iter().find(|uniform| uniform.name() == name)
                else {
                    return Action::none();
                };
                match uniform.value {
                    Type::VecFloat2((x, y)) => {
                        // The smallest of 1, 10, 100, ... that the value fits in.
                        let mut extent = 1.0;
                        while extent < x.abs().max(y.abs()) {
                            extent *= 10.0;
                        }
                        self.editor = Some((name, uniform::Editor::Pad { extent }));
                    }
                    Type::Col3(_) | Type::Col4(_) => {
                        let ([r, g, b, _], _) = uniform.value.components();
                        let hsv = color::Hsv::from_rgb((r as f32, g as f32, b as f32));
                        self.editor = Some((
                            name,
                            uniform::Editor::Color {
                                hue: hsv.hue,
                                saturation: hsv.saturation,
                                hex_text: None,
                            },
                        ));
                    }
                    Type::VecFloat3(vector) => {
                        let direction = split(vector).map_or((0.0, 0.0, 1.0), |(_, unit)| unit);
                        self.editor = Some((name, uniform::Editor::Direction { direction }));
                    }
                    _ => {}
                }
                Action::none()
            }
            Message::Uniform {
                name,
                message: uniform::Message::Pad(point),
            } => {
                self.typing = None;
                let Some((_, uniform::Editor::Pad { extent })) =
                    self.editor.as_ref().filter(|(open, _)| *open == name)
                else {
                    return Action::none();
                };
                let value = Type::VecFloat2((
                    (point.x * 2.0 - 1.0) * extent,
                    (point.y * 2.0 - 1.0) * extent,
                ));
                if self.uniforms.set(&name, value) {
                    Action::instruction(Instruction::Values)
                } else {
                    Action::none()
                }
            }
            Message::Uniform {
                name,
                message:
                    message @ (uniform::Message::Sv(_)
                    | uniform::Message::Hue(_)
                    | uniform::Message::Alpha(_)
                    | uniform::Message::HexChanged(_)
                    | uniform::Message::HexSubmitted),
            } => {
                self.typing = None;
                let Some((
                    _,
                    uniform::Editor::Color {
                        hue,
                        saturation,
                        hex_text,
                    },
                )) = self.editor.as_mut().filter(|(open, _)| *open == name)
                else {
                    return Action::none();
                };
                let Some(value) = self
                    .uniforms
                    .iter()
                    .find(|uniform| uniform.name() == name)
                    .map(|uniform| uniform.value)
                else {
                    return Action::none();
                };
                let ([r, g, b, _], _) = value.components();
                let rgb = (r as f32, g as f32, b as f32);

                // Each path keeps the value's variant, so a `Col3` never gains an alpha.
                let changed = match message {
                    uniform::Message::Sv(point) => {
                        *saturation = point.x;
                        *hex_text = None;
                        let hsv = color::Hsv {
                            hue: *hue,
                            saturation: point.x,
                            value: point.y,
                        };
                        with_color(value, hsv.to_rgb(), None)
                    }
                    uniform::Message::Hue(new) => {
                        *hue = new;
                        *hex_text = None;
                        let hsv = color::Hsv {
                            hue: new,
                            saturation: *saturation,
                            value: color::Hsv::from_rgb(rgb).value,
                        };
                        with_color(value, hsv.to_rgb(), None)
                    }
                    uniform::Message::Alpha(alpha) => {
                        *hex_text = None;
                        value.with_component(3, alpha.into())
                    }
                    uniform::Message::HexChanged(text) => {
                        *hex_text = Some(text);
                        None
                    }
                    uniform::Message::HexSubmitted => {
                        let changed = hex_text
                            .as_deref()
                            .and_then(color::parse_hex)
                            .and_then(|(rgb, alpha)| with_color(value, rgb, alpha));
                        // A refused code stays for the user to correct.
                        if let Some(changed) = changed {
                            follow(changed, hue, saturation);
                            *hex_text = None;
                        }
                        changed
                    }
                    _ => None,
                };
                match changed {
                    Some(changed) if self.uniforms.set(&name, changed) => {
                        Action::instruction(Instruction::Values)
                    }
                    _ => Action::none(),
                }
            }
            Message::Uniform {
                name,
                message:
                    message @ (uniform::Message::Disc(_)
                    | uniform::Message::Flip
                    | uniform::Message::Length(_)),
            } => {
                self.typing = None;
                let Some((_, uniform::Editor::Direction { direction })) =
                    self.editor.as_mut().filter(|(open, _)| *open == name)
                else {
                    return Action::none();
                };
                let Some(Type::VecFloat3(vector)) = self
                    .uniforms
                    .iter()
                    .find(|uniform| uniform.name() == name)
                    .map(|uniform| uniform.value)
                else {
                    return Action::none();
                };
                let length = split(vector).map_or(0.0, |(length, _)| length);

                // The value always joins the length to the kept direction, so neither one is
                // lost when the other changes.
                let length = match message {
                    uniform::Message::Disc(point) => {
                        let (x, y) = (point.x * 2.0 - 1.0, point.y * 2.0 - 1.0);
                        // A point outside the disc moves onto its rim.
                        let radius = x.hypot(y);
                        let (x, y) = (x / radius.max(1.0), y / radius.max(1.0));
                        let radius = radius.min(1.0);
                        // `copysign` keeps the side of a zero `z` too, so the rim doesn't flip it.
                        let z = ((1.0 - radius) * (1.0 + radius))
                            .sqrt()
                            .copysign(direction.2);
                        *direction = (x, y, z);
                        length
                    }
                    uniform::Message::Flip => {
                        direction.2 = -direction.2;
                        length
                    }
                    uniform::Message::Length(new) => new.clamp(0.0, f32::MAX),
                    _ => length,
                };
                if self
                    .uniforms
                    .set(&name, Type::VecFloat3(join(length, *direction)))
                {
                    Action::instruction(Instruction::Values)
                } else {
                    Action::none()
                }
            }
            Message::TypingChanged(text) => {
                if let Some(typing) = &mut self.typing {
                    typing.text = text;
                }
                Action::none()
            }
            Message::Submit => {
                let Some(typing) = self.typing.take() else {
                    return Action::none();
                };
                let Some(uniform) = self
                    .uniforms
                    .iter()
                    .find(|uniform| uniform.name() == typing.name)
                else {
                    return Action::none();
                };
                match typing.value(uniform.value) {
                    Some(value) => {
                        if self.uniforms.set(&typing.name, value) {
                            Action::instruction(Instruction::Values)
                        } else {
                            Action::none()
                        }
                    }
                    // A refused text stays for the user to correct.
                    None => {
                        self.typing = Some(typing);
                        Action::none()
                    }
                }
            }
            Message::CancelTyping => {
                self.typing = None;
                Action::none()
            }
            Message::Close => {
                self.typing = None;
                self.editor = None;
                Action::none()
            }
            Message::Candidate(message) => {
                self.typing = None;
                self.candidate.update(message);
                Action::none()
            }
            Message::Add => {
                self.typing = None;
                match self.candidate.submit(&mut self.uniforms) {
                    Ok(()) => Action::instruction(Instruction::Layout),
                    Err(error) => {
                        tracing::debug!("uniform not added: {error}");
                        Action::none()
                    }
                }
            }
            Message::Remove(name) => {
                self.typing = None;
                if self.editor.as_ref().is_some_and(|(open, _)| *open == name) {
                    self.editor = None;
                }
                if self.uniforms.remove(&name) {
                    Action::instruction(Instruction::Layout)
                } else {
                    Action::none()
                }
            }
            Message::AddTime => {
                self.typing = None;
                self.clock = Some(clock::State::new());
                Action::instruction(Instruction::Layout)
            }
            Message::RemoveTime => {
                self.typing = None;
                self.clock = None;
                Action::instruction(Instruction::Layout)
            }
        };

        // The check depends on the uniforms as well as the typed name.
        self.candidate_check = self.uniforms.check(self.candidate.name());
        action
    }

    pub fn view(&self) -> Element<'_, Message> {
        let time = if let Some(clock) = &self.clock {
            row![
                clock.view().map(Message::Clock),
                button("X").on_press(Message::RemoveTime)
            ]
        } else {
            row![
                space::horizontal(),
                button("Add time").on_press(Message::AddTime)
            ]
        };

        let candidate = row![
            self.candidate
                .view(&self.candidate_check)
                .map(Message::Candidate),
            button("+").on_press_maybe(self.candidate_check.is_ok().then_some(Message::Add)),
        ];

        let uniforms = column(self.uniforms.iter().map(|value| {
            let typing = self
                .typing
                .as_ref()
                .filter(|typing| typing.name == value.name());
            let editor = self
                .editor
                .as_ref()
                .filter(|(name, _)| name == value.name())
                .map(|(_, editor)| editor);
            row![
                uniform::view(value, typing, editor),
                button(text("×").center().width(Fill).height(Fill))
                    .on_press(Message::Remove(value.name().to_owned()))
                    .style(button::text)
                    .padding(0)
                    .width(ROW_HEIGHT)
                    .height(ROW_HEIGHT),
            ]
            .into()
        }));

        column![time, candidate, uniforms].into()
    }

    pub fn uniforms(&self) -> &Uniforms {
        &self.uniforms
    }

    /// The clock behind `time`, when it is on.
    pub fn clock(&self) -> Option<Clock> {
        self.clock.as_ref().map(clock::State::clock)
    }

    /// Seconds on the clock, when `time` is on.
    pub fn time(&self) -> Option<f32> {
        self.clock.as_ref().map(clock::State::seconds)
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let clock = if let Some(clock) = &self.clock {
            clock.subscription().map(Message::Clock)
        } else {
            Subscription::none()
        };
        // The text input doesn't capture Escape, so it reaches this listener.
        let escape = if self.typing.is_some() {
            keyboard::listen().filter_map(|event| match event {
                keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(key::Named::Escape),
                    ..
                } => Some(Message::CancelTyping),
                _ => None,
            })
        } else {
            Subscription::none()
        };

        Subscription::batch([clock, escape])
    }
}

impl Typing {
    /// `value` with the typed component set to the text, or `None` when the text isn't a number
    /// the component can hold.
    fn value(&self, value: Type) -> Option<Type> {
        let component = self.text.trim().parse().ok()?;
        value.with_component(self.component, component)
    }
}

/// `value`, a color, with its channels set to `rgb`, and its alpha too when there is one, or
/// `None` when it can't hold them, as a `Col3` can't hold an alpha.
fn with_color(value: Type, (r, g, b): (f32, f32, f32), alpha: Option<f32>) -> Option<Type> {
    let value = value
        .with_component(0, r.into())?
        .with_component(1, g.into())?
        .with_component(2, b.into())?;
    match alpha {
        Some(alpha) => value.with_component(3, alpha.into()),
        None => Some(value),
    }
}

/// Takes `hue` and `saturation` from the color `value`, unless it has none to give: black keeps
/// both, and a grey keeps the hue.
fn follow(value: Type, hue: &mut f32, saturation: &mut f32) {
    let ([r, g, b, _], _) = value.components();
    let hsv = color::Hsv::from_rgb((r as f32, g as f32, b as f32));
    if hsv.value > 0.0 {
        if hsv.saturation > 0.0 {
            *hue = hsv.hue;
        }
        *saturation = hsv.saturation;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(state: &mut State, name: &str) -> Option<Instruction> {
        state
            .update(Message::Candidate(uniform::CandidateMessage::NameChanged(
                name.to_owned(),
            )))
            .instruction
    }

    fn set(name: &str, value: Type) -> Message {
        Message::Uniform {
            name: name.to_owned(),
            message: uniform::Message::Set(value),
        }
    }

    /// A state with one uniform, `name`, typing `text` into its component `component`.
    fn typing(name: &str, value: Type, component: usize, text: &str) -> State {
        let mut uniforms = Uniforms::default();
        uniforms.add(name, value).unwrap();
        let mut state = State::new(uniforms, None);
        let _ = state.update(Message::Uniform {
            name: name.to_owned(),
            message: uniform::Message::StartTyping(component),
        });
        let _ = state.update(Message::TypingChanged(text.to_owned()));
        state
    }

    fn value(state: &State) -> Type {
        state.uniforms().iter().next().unwrap().value
    }

    #[test]
    fn adding_a_uniform_changes_the_layout() {
        let mut state = State::new(Uniforms::default(), None);

        assert!(name(&mut state, "radius").is_none());
        assert!(matches!(
            state.update(Message::Add).instruction,
            Some(Instruction::Layout)
        ));
        assert_eq!(state.uniforms().iter().count(), 1);
    }

    #[test]
    fn a_taken_name_is_not_added() {
        let mut state = State::new(Uniforms::default(), None);
        name(&mut state, "radius");
        let _ = state.update(Message::Add);
        name(&mut state, "radius");

        assert!(state.update(Message::Add).instruction.is_none());
        assert_eq!(state.uniforms().iter().count(), 1);
    }

    #[test]
    fn a_removed_name_can_be_added_again() {
        let mut state = State::new(Uniforms::default(), None);
        name(&mut state, "radius");
        let _ = state.update(Message::Add);
        name(&mut state, "radius");
        assert!(state.candidate_check.is_err());

        let _ = state.update(Message::Remove("radius".to_owned()));

        assert!(state.candidate_check.is_ok());
    }

    #[test]
    fn removing_an_unknown_uniform_changes_nothing() {
        let mut state = State::new(Uniforms::default(), None);

        assert!(
            state
                .update(Message::Remove("radius".to_owned()))
                .instruction
                .is_none()
        );
    }

    #[test]
    fn setting_a_value_changes_only_values() {
        let mut uniforms = Uniforms::default();
        uniforms.add("radius", Type::Float(0.5)).unwrap();
        let mut state = State::new(uniforms, None);

        let instruction = state.update(set("radius", Type::Float(0.7))).instruction;

        assert!(matches!(instruction, Some(Instruction::Values)));
        let radius = state.uniforms().iter().next().unwrap();
        assert_eq!(radius.value, Type::Float(0.7));
    }

    #[test]
    fn turning_time_on_and_off_changes_the_layout() {
        let mut state = State::new(Uniforms::default(), None);

        assert!(matches!(
            state.update(Message::AddTime).instruction,
            Some(Instruction::Layout)
        ));
        assert_eq!(state.time(), Some(0.0));
        assert!(matches!(
            state.update(Message::RemoveTime).instruction,
            Some(Instruction::Layout)
        ));
        assert_eq!(state.time(), None);
    }

    #[test]
    fn a_click_starts_typing_with_the_full_value() {
        let mut uniforms = Uniforms::default();
        uniforms
            .add("offset", Type::VecFloat2((0.5, 0.123_456)))
            .unwrap();
        let mut state = State::new(uniforms, None);

        let _ = state.update(Message::Uniform {
            name: "offset".to_owned(),
            message: uniform::Message::StartTyping(1),
        });

        let typing = state.typing.as_ref().unwrap();
        assert_eq!(typing.name, "offset");
        assert_eq!(typing.component, 1);
        assert_eq!(typing.text, "0.123456");
    }

    #[test]
    fn submitting_a_different_value_sets_it_and_ends_typing() {
        let mut state = typing("offset", Type::VecFloat2((0.5, 0.5)), 1, " -2.25 ");

        let instruction = state.update(Message::Submit).instruction;

        assert!(matches!(instruction, Some(Instruction::Values)));
        assert_eq!(value(&state), Type::VecFloat2((0.5, -2.25)));
        assert!(state.typing.is_none());
    }

    #[test]
    fn submitting_the_same_value_ends_typing_without_an_instruction() {
        let mut state = typing("count", Type::Int(3), 0, "3");

        assert!(state.update(Message::Submit).instruction.is_none());
        assert!(state.typing.is_none());
    }

    #[test]
    fn a_refused_submit_keeps_typing_and_changes_nothing() {
        let cases = [
            (Type::Float(0.5), "abc"),
            (Type::Float(0.5), "inf"),
            (Type::Float(0.5), "NaN"),
            (Type::Float(0.5), "1e40"),
            (Type::Int(3), "2.5"),
            (Type::Int(3), "3000000000"),
            (Type::Col3((0.5, 0.5, 0.5)), "1.5"),
        ];

        for (start, text) in cases {
            let mut state = typing("u", start, 0, text);

            assert!(state.update(Message::Submit).instruction.is_none());
            assert_eq!(value(&state), start, "{text}");
            assert_eq!(state.typing.as_ref().unwrap().text, text);
        }
    }

    #[test]
    fn escape_cancels_typing() {
        let mut state = typing("radius", Type::Float(0.5), 0, "2");

        assert!(state.update(Message::CancelTyping).instruction.is_none());
        assert!(state.typing.is_none());
        assert_eq!(value(&state), Type::Float(0.5));
    }

    #[test]
    fn removing_the_typed_uniform_clears_typing() {
        let mut state = typing("radius", Type::Float(0.5), 0, "2");

        let _ = state.update(Message::Remove("radius".to_owned()));

        assert!(state.typing.is_none());
    }

    fn uniform(name: &str, message: uniform::Message) -> Message {
        Message::Uniform {
            name: name.to_owned(),
            message,
        }
    }

    /// A state with the uniforms `names`, each the vec2 `value`.
    fn vec2s(names: &[&str], value: (f32, f32)) -> State {
        let mut uniforms = Uniforms::default();
        for name in names {
            uniforms.add(name, Type::VecFloat2(value)).unwrap();
        }
        State::new(uniforms, None)
    }

    #[test]
    fn opening_and_closing_a_vec2_shows_and_hides_its_pad() {
        let mut state = vec2s(&["offset"], (0.5, 0.5));

        let _ = state.update(uniform("offset", uniform::Message::Open));
        assert_eq!(
            state.editor,
            Some(("offset".to_owned(), uniform::Editor::Pad { extent: 1.0 }))
        );

        let _ = state.update(Message::Close);
        assert!(state.editor.is_none());
    }

    #[test]
    fn opening_another_uniform_replaces_the_open_one() {
        let mut state = vec2s(&["offset", "center"], (0.5, 0.5));

        let _ = state.update(uniform("offset", uniform::Message::Open));
        let _ = state.update(uniform("center", uniform::Message::Open));

        assert_eq!(state.editor.unwrap().0, "center");
    }

    #[test]
    fn removing_the_open_uniform_closes_it() {
        let mut state = vec2s(&["offset", "center"], (0.5, 0.5));
        let _ = state.update(uniform("offset", uniform::Message::Open));

        let _ = state.update(Message::Remove("center".to_owned()));
        assert!(state.editor.is_some());

        let _ = state.update(Message::Remove("offset".to_owned()));
        assert!(state.editor.is_none());
    }

    #[test]
    fn the_pad_s_extent_is_the_power_of_ten_covering_the_value() {
        for (component, extent) in [(0.5, 1.0), (7.0, 10.0), (-42.0, 100.0)] {
            let mut state = vec2s(&["offset"], (0.0, component));

            let _ = state.update(uniform("offset", uniform::Message::Open));

            assert_eq!(
                state.editor.unwrap().1,
                uniform::Editor::Pad { extent },
                "{component}"
            );
        }
    }

    #[test]
    fn a_pad_message_sets_the_value_inside_the_extent() {
        let mut state = vec2s(&["offset"], (7.0, 0.0));
        let _ = state.update(uniform("offset", uniform::Message::Open));

        let instruction = state
            .update(uniform(
                "offset",
                uniform::Message::Pad(iced::Point::new(0.75, 0.0)),
            ))
            .instruction;

        assert!(matches!(instruction, Some(Instruction::Values)));
        assert_eq!(value(&state), Type::VecFloat2((5.0, -10.0)));
    }
    /// A state with the color `value` named `tint` and its picker open.
    fn picking(value: Type) -> State {
        let mut uniforms = Uniforms::default();
        uniforms.add("tint", value).unwrap();
        let mut state = State::new(uniforms, None);
        let _ = state.update(uniform("tint", uniform::Message::Open));
        state
    }

    fn hue(state: &State) -> f32 {
        match state.editor {
            Some((_, uniform::Editor::Color { hue, .. })) => hue,
            _ => panic!("no color picker is open"),
        }
    }

    #[test]
    fn opening_a_color_stores_its_hue() {
        let state = picking(Type::Col3((0.0, 0.0, 1.0)));

        assert!((hue(&state) - 2.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn dragging_to_black_keeps_the_hue() {
        let mut state = picking(Type::Col3((0.0, 0.0, 1.0)));
        let blue = hue(&state);

        let _ = state.update(uniform(
            "tint",
            uniform::Message::Sv(iced::Point::new(1.0, 0.0)),
        ));
        assert_eq!(value(&state), Type::Col3((0.0, 0.0, 0.0)));
        assert_eq!(hue(&state), blue);

        let _ = state.update(uniform(
            "tint",
            uniform::Message::Sv(iced::Point::new(1.0, 1.0)),
        ));
        assert_eq!(value(&state), Type::Col3((0.0, 0.0, 1.0)));
    }

    #[test]
    fn setting_a_grey_keeps_the_hue() {
        let mut state = picking(Type::Col3((0.0, 0.0, 1.0)));
        let blue = hue(&state);

        let _ = state.update(set("tint", Type::Col3((0.5, 0.5, 0.5))));

        assert_eq!(hue(&state), blue);
    }

    #[test]
    fn submitting_a_hex_code_sets_the_color() {
        let mut state = picking(Type::Col4((0.0, 0.0, 0.0, 0.25)));

        let _ = state.update(uniform(
            "tint",
            uniform::Message::HexChanged("#ff0000".to_owned()),
        ));
        let instruction = state
            .update(uniform("tint", uniform::Message::HexSubmitted))
            .instruction;

        assert!(matches!(instruction, Some(Instruction::Values)));
        assert_eq!(value(&state), Type::Col4((1.0, 0.0, 0.0, 0.25)));
        assert!(matches!(
            state.editor,
            Some((_, uniform::Editor::Color { hex_text: None, .. }))
        ));
    }

    #[test]
    fn a_refused_hex_code_changes_nothing() {
        for (start, code) in [
            (Type::Col3((0.5, 0.5, 0.5)), "#12345"),
            (Type::Col3((0.5, 0.5, 0.5)), "#ff000080"),
            (Type::Col4((0.5, 0.5, 0.5, 0.5)), "red"),
        ] {
            let mut state = picking(start);
            let _ = state.update(uniform(
                "tint",
                uniform::Message::HexChanged(code.to_owned()),
            ));

            let instruction = state
                .update(uniform("tint", uniform::Message::HexSubmitted))
                .instruction;

            assert!(instruction.is_none(), "{code}");
            assert_eq!(value(&state), start, "{code}");
            assert!(
                matches!(
                    &state.editor,
                    Some((_, uniform::Editor::Color { hex_text: Some(text), .. })) if text == code
                ),
                "{code}"
            );
        }
    }

    #[test]
    fn a_col3_stays_a_col3_through_every_message() {
        let mut state = picking(Type::Col3((0.2, 0.4, 0.6)));

        for message in [
            uniform::Message::Sv(iced::Point::new(0.5, 0.5)),
            uniform::Message::Hue(0.25),
            uniform::Message::Alpha(0.5),
            uniform::Message::HexChanged("#ff000080".to_owned()),
            uniform::Message::HexSubmitted,
            uniform::Message::HexChanged("#00ff00".to_owned()),
            uniform::Message::HexSubmitted,
            uniform::Message::Set(Type::Col3((0.1, 0.2, 0.3))),
        ] {
            let _ = state.update(uniform("tint", message));

            assert!(matches!(value(&state), Type::Col3(_)));
        }
        assert_eq!(value(&state), Type::Col3((0.1, 0.2, 0.3)));
    }

    /// A state with the vec3 `value` named `normal` and its direction editor open.
    fn pointing(value: (f32, f32, f32)) -> State {
        let mut uniforms = Uniforms::default();
        uniforms.add("normal", Type::VecFloat3(value)).unwrap();
        let mut state = State::new(uniforms, None);
        let _ = state.update(uniform("normal", uniform::Message::Open));
        state
    }

    fn assert_near(state: &State, expected: (f32, f32, f32)) {
        let Type::VecFloat3((x, y, z)) = value(state) else {
            panic!("not a vec3");
        };
        let (ex, ey, ez) = expected;
        assert!(
            (x - ex).abs() < 1e-5 && (y - ey).abs() < 1e-5 && (z - ez).abs() < 1e-5,
            "{:?} is not {expected:?}",
            (x, y, z)
        );
    }

    #[test]
    fn opening_a_zero_vec3_points_it_to_the_front() {
        let state = pointing((0.0, 0.0, 0.0));

        assert_eq!(
            state.editor.unwrap().1,
            uniform::Editor::Direction {
                direction: (0.0, 0.0, 1.0)
            }
        );
    }

    #[test]
    fn a_zero_length_keeps_the_chosen_direction() {
        let mut state = pointing((0.0, 0.0, 2.0));

        let _ = state.update(uniform("normal", uniform::Message::Length(0.0)));
        assert_eq!(value(&state), Type::VecFloat3((0.0, 0.0, 0.0)));
        let _ = state.update(uniform(
            "normal",
            uniform::Message::Disc(iced::Point::new(0.75, 0.5)),
        ));
        let _ = state.update(uniform("normal", uniform::Message::Length(2.0)));

        assert_near(&state, (1.0, 0.0, 3.0_f32.sqrt()));
    }

    #[test]
    fn the_rim_keeps_the_back() {
        let mut state = pointing((0.0, 0.0, -1.0));

        let _ = state.update(uniform(
            "normal",
            uniform::Message::Disc(iced::Point::new(1.0, 0.5)),
        ));
        assert_near(&state, (1.0, 0.0, 0.0));
        let _ = state.update(uniform(
            "normal",
            uniform::Message::Disc(iced::Point::new(0.5, 0.5)),
        ));

        assert_near(&state, (0.0, 0.0, -1.0));
    }

    #[test]
    fn flipping_changes_only_z() {
        let mut state = pointing((3.0, 0.0, 4.0));

        let instruction = state
            .update(uniform("normal", uniform::Message::Flip))
            .instruction;

        assert!(matches!(instruction, Some(Instruction::Values)));
        assert_near(&state, (3.0, 0.0, -4.0));
    }

    #[test]
    fn a_point_outside_the_disc_is_clamped_onto_it() {
        let mut state = pointing((0.0, 0.0, 2.0));

        let _ = state.update(uniform(
            "normal",
            uniform::Message::Disc(iced::Point::new(1.0, 1.0)),
        ));

        assert_near(&state, (2.0_f32.sqrt(), 2.0_f32.sqrt(), 0.0));
    }

    #[test]
    fn setting_a_vec3_follows_its_direction_unless_it_is_zero() {
        let mut state = pointing((0.0, 0.0, 1.0));

        let _ = state.update(set("normal", Type::VecFloat3((0.0, -2.0, 0.0))));
        let _ = state.update(set("normal", Type::VecFloat3((0.0, 0.0, 0.0))));

        assert_eq!(
            state.editor.unwrap().1,
            uniform::Editor::Direction {
                direction: (0.0, -1.0, 0.0)
            }
        );
    }
}
