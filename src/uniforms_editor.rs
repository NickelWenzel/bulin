mod time;
pub mod uniform;

use crate::pipeline_update::*;
use std::time::Instant;
use uniform::*;

use iced::{
    widget::{button, container, horizontal_space, row},
    Element, Task,
};
use serde::{Deserialize, Serialize};
use std::vec::Vec;

#[derive(Debug, Clone)]
pub enum Message {
    Time(time::Message),
    Uniforms(u32, uniform::Message),
    Candidate(uniform::CandidateMessage),
    Update(PipelineUpdate),
    AddTime,
    RemoveTime,
    AddUniform(Uniform),
    RemoveUniform(u32),
}

#[derive(Serialize, Deserialize)]
pub struct UniformsEditor {
    uniforms: Vec<Uniform>,
    #[serde(skip, default = "Candidate::default")]
    candidate: Candidate,
    time: Option<time::Time>,
}

impl UniformsEditor {
    pub fn new() -> Self {
        Self {
            uniforms: Vec::new(),
            candidate: Candidate::new(),
            time: Option::None,
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AddTime => {
                let now = Instant::now();
                self.time = Some(time::Time::new(now));
                Task::done(Message::Update(PipelineUpdate::Time(TimeUpdate::Add(now))))
            }
            Message::RemoveTime => {
                self.time = Option::None;
                Task::done(Message::Update(PipelineUpdate::Time(TimeUpdate::Remove)))
            }
            Message::Time(message) => {
                if let Some(time) = &mut self.time {
                    time.update(message).map(Message::Time)
                } else {
                    Task::none()
                }
            }
            Message::AddUniform(uniform) => {
                self.uniforms.push(uniform.clone());
                Task::done(Message::Update(PipelineUpdate::Uniforms(
                    UniformsUpdate::Add(uniform),
                )))
            }
            Message::RemoveUniform(idx) => {
                self.uniforms.swap_remove(idx as usize);
                Task::done(Message::Update(PipelineUpdate::Uniforms(
                    UniformsUpdate::Remove(idx),
                )))
            }
            Message::Uniforms(idx, message) => {
                if let Some(uniform) = self.uniforms.get_mut(idx as usize) {
                    uniform
                        .update(message)
                        .map(move |m| Message::Uniforms(idx, m))
                        .chain(Task::done(Message::Update(PipelineUpdate::Uniforms(
                            UniformsUpdate::Update(idx, uniform.clone()),
                        ))))
                } else {
                    Task::none()
                }
            }
            Message::Candidate(message) => self.candidate.update(message).map(Message::Candidate),
            Message::Update(_) => Task::none(),
        }
    }

    pub fn view(&self) -> Element<Message> {
        let time = if let Some(time) = &self.time {
            row![
                time.view().map(Message::Time),
                button("X").on_press(Message::RemoveTime)
            ]
        } else {
            row![horizontal_space(), button("+").on_press(Message::AddTime)]
        };

        let add_candidate_button = if let Ok(uniform) = self.candidate.clone().try_into() {
            button("+").on_press(Message::AddUniform(uniform))
        } else {
            button("+")
        };

        let candidate = row![
            self.candidate.view().map(Message::Candidate),
            add_candidate_button,
        ];

        let uniforms = iced::widget::Column::from_vec(
            self.uniforms
                .iter()
                .enumerate()
                .map(|(idx, u)| {
                    row![
                        u.view().map(move |m| Message::Uniforms(idx as u32, m)),
                        button("X").on_press(Message::RemoveUniform(idx as u32)),
                    ]
                    .into()
                })
                .collect(),
        );
        iced::widget::column![time, candidate, uniforms].into()
    }
}
