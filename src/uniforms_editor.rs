mod time;
pub mod uniform;

use crate::pipeline_update::*;
use uniform::*;

use iced::{
    widget::{button, horizontal_space, row},
    Element, Subscription, Task,
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
    #[serde(skip, default = "Candidate::new")]
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
                self.time = Some(time::Time::new());
                Task::done(Message::Update(PipelineUpdate::Uniforms(
                    UniformsUpdate::Add(Uniform {
                        value: Type::Float(0.0),
                        name: String::from("time"),
                    }),
                )))
            }
            Message::RemoveTime => {
                self.time = Option::None;
                Task::done(Message::Update(PipelineUpdate::Uniforms(
                    UniformsUpdate::Remove(String::from("time")),
                )))
            }
            Message::Time(message) => {
                if let Some(time) = &mut self.time {
                    time.update(message)
                        .map(Message::Time)
                        .chain(Task::done(Message::Update(PipelineUpdate::Uniforms(
                            UniformsUpdate::Update(
                                String::from("time"),
                                Uniform {
                                    value: Type::Float(time.duration() as f64),
                                    name: String::from("time"),
                                },
                            ),
                        ))))
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
                if let Some(name) = self.get_name(idx) {
                    self.uniforms.remove(idx as usize);
                    Task::done(Message::Update(PipelineUpdate::Uniforms(
                        UniformsUpdate::Remove(name),
                    )))
                } else {
                    return Task::none();
                }
            }
            Message::Uniforms(idx, message) => {
                if let Some(uniform) = self.uniforms.get_mut(idx as usize) {
                    let name = uniform.name.clone();
                    uniform
                        .update(message)
                        .map(move |m| Message::Uniforms(idx, m))
                        .chain(Task::done(Message::Update(PipelineUpdate::Uniforms(
                            UniformsUpdate::Update(name, uniform.clone()),
                        ))))
                } else {
                    Task::none()
                }
            }
            Message::Candidate(message) => self.candidate.update(message).map(Message::Candidate),
            Message::Update(_) => Task::none(),
        }
    }

    fn get_name(&self, idx: u32) -> Option<String> {
        self.uniforms.get(idx as usize).map(|u| u.name.clone())
    }

    pub fn view(&self) -> Element<Message> {
        let time = if let Some(time) = &self.time {
            row![
                time.view().map(Message::Time),
                button("X").on_press(Message::RemoveTime)
            ]
        } else {
            row![
                horizontal_space(),
                button("Add time").on_press(Message::AddTime)
            ]
        };

        let candidate = row![
            self.candidate.view().map(Message::Candidate),
            if let Ok(uniform) = self.candidate.clone().try_into() {
                button("+").on_press(Message::AddUniform(uniform))
            } else {
                button("+")
            },
        ];

        let uniforms =
            iced::widget::Column::from_iter(self.uniforms.iter().enumerate().map(|(idx, u)| {
                row![
                    u.view().map(move |m| Message::Uniforms(idx as u32, m)),
                    button("X").on_press(Message::RemoveUniform(idx as u32)),
                ]
                .into()
            }));
        iced::widget::column![time, candidate, uniforms].into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        if let Some(time) = &self.time {
            time.subscription().map(Message::Time)
        } else {
            Subscription::none()
        }
    }
}
