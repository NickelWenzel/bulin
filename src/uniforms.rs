//! The uniforms editor: the user's uniforms and the clock behind `time`.

mod clock;
mod uniform;

use iced::keyboard::{self, key};
use iced::widget::{self, button, column, operation, row, space, text};
use iced::{Element, Fill, Subscription, Task};

use crate::action::Action;
use crate::core::uniform::{Error, Type};
use crate::core::{Clock, Uniforms};

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
                if self.uniforms.set(&name, value) {
                    Action::instruction(Instruction::Values)
                } else {
                    Action::none()
                }
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
                if let Type::VecFloat2((x, y)) = uniform.value {
                    // The smallest of 1, 10, 100, ... that the value fits in.
                    let mut extent = 1.0;
                    while extent < x.abs().max(y.abs()) {
                        extent *= 10.0;
                    }
                    self.editor = Some((name, uniform::Editor::Pad { extent }));
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
}
