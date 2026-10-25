//! The uniforms editor: the user's uniforms and the clock behind `time`.

mod clock;
mod uniform;

use iced::widget::{button, column, row, space};
use iced::{Element, Subscription};

use crate::core::uniform::Type;
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
    /// A uniform or the clock changed.
    Changed,
}

pub struct State {
    uniforms: Uniforms,
    clock: Option<clock::State>,
    candidate: uniform::Candidate,
}

impl State {
    pub fn new(uniforms: Uniforms, clock: Option<Clock>) -> Self {
        Self {
            uniforms,
            clock: clock.map(clock::State::from),
            candidate: uniform::Candidate::new(),
        }
    }

    pub fn update(&mut self, message: Message) -> Option<Instruction> {
        match message {
            Message::Clock(message) => {
                let Some(clock) = &mut self.clock else {
                    return None;
                };
                clock.update(message);
            }
            Message::Set(name, value) => self.uniforms.set(&name, value),
            Message::Candidate(message) => {
                self.candidate.update(message);
                return None;
            }
            Message::Add => {
                if let Err(error) = self
                    .uniforms
                    .add(&self.candidate.name, self.candidate.value)
                {
                    tracing::debug!("uniform not added: {error}");
                    return None;
                }
                self.candidate.name.clear();
            }
            Message::Remove(name) => self.uniforms.remove(&name),
            Message::AddTime => self.clock = Some(clock::State::new()),
            Message::RemoveTime => self.clock = None,
        }

        Some(Instruction::Changed)
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

        let addable = self.uniforms.check(&self.candidate.name).is_ok();
        let candidate = row![
            self.candidate.view(&self.uniforms).map(Message::Candidate),
            button("+").on_press_maybe(addable.then_some(Message::Add)),
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
    fn adding_a_uniform_changes_the_program() {
        let mut state = State::new(Uniforms::default(), None);

        assert!(name(&mut state, "radius").is_none());
        assert!(matches!(
            state.update(Message::Add),
            Some(Instruction::Changed)
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
    fn moving_a_slider_changes_the_program() {
        let mut uniforms = Uniforms::default();
        uniforms.add("radius", Type::Float(0.5)).unwrap();
        let mut state = State::new(uniforms, None);

        let instruction = state.update(Message::Set("radius".to_owned(), Type::Float(0.7)));

        assert!(matches!(instruction, Some(Instruction::Changed)));
        let radius = state.uniforms().iter().next().unwrap();
        assert_eq!(radius.value, Type::Float(0.7));
    }

    #[test]
    fn turning_time_on_and_off_changes_the_program() {
        let mut state = State::new(Uniforms::default(), None);

        assert!(matches!(
            state.update(Message::AddTime),
            Some(Instruction::Changed)
        ));
        assert_eq!(state.time(), Some(0.0));
        assert!(matches!(
            state.update(Message::RemoveTime),
            Some(Instruction::Changed)
        ));
        assert_eq!(state.time(), None);
    }
}
