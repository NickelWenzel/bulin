//! The uniforms editor: the user's uniforms and the clock behind `time`.

mod clock;
mod uniform;

use iced::widget::{button, column, row, space};
use iced::{Element, Subscription};

use crate::core::uniform::{Error, Type};
use crate::core::{Clock, Uniforms};

#[derive(Debug, Clone)]
pub enum Message {
    Clock(clock::Message),
    Set(String, Type),
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
}

impl State {
    pub fn new(uniforms: Uniforms, clock: Option<Clock>) -> Self {
        let candidate = uniform::Candidate::new();
        Self {
            candidate_check: uniforms.check(candidate.name()),
            uniforms,
            clock: clock.map(clock::State::from),
            candidate,
        }
    }

    pub fn update(&mut self, message: Message) -> Option<Instruction> {
        let instruction = match message {
            Message::Clock(message) => self.clock.as_mut().map(|clock| {
                clock.update(message);
                Instruction::Values
            }),
            Message::Set(name, value) => self
                .uniforms
                .set(&name, value)
                .then_some(Instruction::Values),
            Message::Candidate(message) => {
                self.candidate.update(message);
                None
            }
            Message::Add => match self.candidate.submit(&mut self.uniforms) {
                Ok(()) => Some(Instruction::Layout),
                Err(error) => {
                    tracing::debug!("uniform not added: {error}");
                    None
                }
            },
            Message::Remove(name) => self.uniforms.remove(&name).then_some(Instruction::Layout),
            Message::AddTime => {
                self.clock = Some(clock::State::new());
                Some(Instruction::Layout)
            }
            Message::RemoveTime => {
                self.clock = None;
                Some(Instruction::Layout)
            }
        };

        // The check depends on the uniforms as well as the typed name.
        self.candidate_check = self.uniforms.check(self.candidate.name());
        instruction
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
            let name = value.name().to_owned();
            row![
                uniform::view(value).map(move |value| Message::Set(name.clone(), value)),
                button("X").on_press(Message::Remove(value.name().to_owned())),
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
        if let Some(clock) = &self.clock {
            clock.subscription().map(Message::Clock)
        } else {
            Subscription::none()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(state: &mut State, name: &str) -> Option<Instruction> {
        state.update(Message::Candidate(uniform::CandidateMessage::NameChanged(
            name.to_owned(),
        )))
    }

    #[test]
    fn adding_a_uniform_changes_the_layout() {
        let mut state = State::new(Uniforms::default(), None);

        assert!(name(&mut state, "radius").is_none());
        assert!(matches!(
            state.update(Message::Add),
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

        assert!(state.update(Message::Add).is_none());
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

        assert!(state.update(Message::Remove("radius".to_owned())).is_none());
    }

    #[test]
    fn moving_a_slider_changes_only_values() {
        let mut uniforms = Uniforms::default();
        uniforms.add("radius", Type::Float(0.5)).unwrap();
        let mut state = State::new(uniforms, None);

        let instruction = state.update(Message::Set("radius".to_owned(), Type::Float(0.7)));

        assert!(matches!(instruction, Some(Instruction::Values)));
        let radius = state.uniforms().iter().next().unwrap();
        assert_eq!(radius.value, Type::Float(0.7));
    }

    #[test]
    fn turning_time_on_and_off_changes_the_layout() {
        let mut state = State::new(Uniforms::default(), None);

        assert!(matches!(
            state.update(Message::AddTime),
            Some(Instruction::Layout)
        ));
        assert_eq!(state.time(), Some(0.0));
        assert!(matches!(
            state.update(Message::RemoveTime),
            Some(Instruction::Layout)
        ));
        assert_eq!(state.time(), None);
    }
}
