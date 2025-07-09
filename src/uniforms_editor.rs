mod time;
pub mod uniform;

use crate::shader_update::*;
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
    Uniforms(String, uniform::Message),
    Candidate(uniform::CandidateMessage),
    Update(ShaderUpdate),
    AddTime,
    RemoveTime,
    AddUniform(EditorUniform),
    RemoveUniform(String),
}

#[derive(Serialize, Deserialize)]
pub struct UniformsEditor {
    uniforms: Vec<EditorUniform>,
    #[serde(skip, default = "Candidate::new")]
    candidate: Candidate,
    time: Option<time::Time>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EditorUniform {
    value: Uniform,
    visible: bool,
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
                Task::done(Message::AddUniform(EditorUniform {
                    value: Uniform {
                        value: Type::Float(0.0),
                        name: String::from("time"),
                    },
                    visible: false,
                }))
            }
            Message::RemoveTime => {
                self.time = Option::None;
                Task::done(Message::RemoveUniform("time".to_string()))
            }
            Message::Time(message) => {
                if let Some(time) = &mut self.time {
                    time.update(message)
                        .map(Message::Time)
                        .chain(Task::done(Message::Update(ShaderUpdate::Uniforms(
                            UniformsUpdate::Update(
                                "time".to_string(),
                                Uniform {
                                    value: Type::Float(time.duration()),
                                    name: "time".to_string(),
                                },
                            ),
                        ))))
                } else {
                    Task::none()
                }
            }
            Message::AddUniform(uniform) => {
                self.uniforms.push(uniform.clone());
                Task::done(Message::Update(ShaderUpdate::Uniforms(
                    UniformsUpdate::Add(uniform.value),
                )))
            }
            Message::RemoveUniform(name) => {
                if let Some(idx) = self.uniforms.iter().position(|u| u.value.name == name) {
                    self.uniforms.remove(idx);
                    Task::done(Message::Update(ShaderUpdate::Uniforms(
                        UniformsUpdate::Remove(name),
                    )))
                } else {
                    return Task::none();
                }
            }
            Message::Uniforms(name, message) => {
                if let Some(uniform) = self.uniforms.iter_mut().find(|u| u.value.name == name) {
                    let name_c = name.clone();
                    uniform
                        .value
                        .update(message)
                        .map(move |m| Message::Uniforms(name.clone(), m))
                        .chain(Task::done(Message::Update(ShaderUpdate::Uniforms(
                            UniformsUpdate::Update(name_c, uniform.value.clone()),
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
            row![
                horizontal_space(),
                button("Add time").on_press(Message::AddTime)
            ]
        };

        let candidate = row![
            self.candidate.view().map(Message::Candidate),
            if let Ok(uniform) = self.candidate.clone().try_into() {
                button("+").on_press(Message::AddUniform(EditorUniform {
                    value: uniform,
                    visible: true,
                }))
            } else {
                button("+")
            },
        ];

        let uniforms = iced::widget::Column::from_iter(self.uniforms.iter().filter_map(|u| {
            if u.visible {
                Some(
                    row![
                        u.value
                            .view()
                            .map(|m| Message::Uniforms(u.value.name.clone(), m)),
                        button("X").on_press(Message::RemoveUniform(u.value.name.clone())),
                    ]
                    .into(),
                )
            } else {
                None
            }
        }));
        iced::widget::column![time, candidate, uniforms].into()
    }

    pub fn uniforms(&self) -> Vec<Uniform> {
        self.uniforms.iter().map(|u| u.value.clone()).collect()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        if let Some(time) = &self.time {
            time.subscription().map(Message::Time)
        } else {
            Subscription::none()
        }
    }
}
