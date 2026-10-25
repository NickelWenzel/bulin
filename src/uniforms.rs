//! The uniforms editor: the user's uniforms and the clock behind `time`.

mod clock;
mod uniform;

use iced::widget::{button, column, row, space};
use iced::{Element, Subscription, Task};
use serde::{Deserialize, Serialize};

use crate::core::Uniforms;
use crate::core::uniform::Type;

#[derive(Debug, Clone)]
pub enum Message {
    Clock(clock::Message),
    Set(String, Type),
    Candidate(uniform::CandidateMessage),
    Add,
    Remove(String),
    AddTime,
    RemoveTime,
    Changed,
}

#[derive(Serialize, Deserialize)]
pub struct State {
    uniforms: Uniforms,
    #[serde(rename = "time")]
    clock: Option<clock::State>,
    #[serde(skip)]
    candidate: uniform::Candidate,
}

impl State {
    pub fn new() -> Self {
        Self {
            uniforms: Uniforms::default(),
            clock: None,
            candidate: uniform::Candidate::new(),
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Clock(message) => {
                let Some(clock) = &mut self.clock else {
                    return Task::none();
                };
                clock.update(message);
            }
            Message::Set(name, value) => self.uniforms.set(&name, value),
            Message::Candidate(message) => {
                self.candidate.update(message);
                return Task::none();
            }
            Message::Add => {
                if let Err(error) = self
                    .uniforms
                    .add(&self.candidate.name, self.candidate.value)
                {
                    tracing::debug!("uniform not added: {error}");
                    return Task::none();
                }
                self.candidate.name.clear();
            }
            Message::Remove(name) => self.uniforms.remove(&name),
            Message::AddTime => self.clock = Some(clock::State::new()),
            Message::RemoveTime => self.clock = None,
            Message::Changed => return Task::none(),
        }

        Task::done(Message::Changed)
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
