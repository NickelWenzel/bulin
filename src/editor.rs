use crate::pipeline_update::PipelineUpdate;
use crate::text_editor;
use crate::uniforms_editor;

use iced::widget::container;
use iced::Subscription;
use iced::{Element, Task};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub enum Message {
    TextEditor(text_editor::Message),
    UniformsEditor(uniforms_editor::Message),
    UpdatePipeline(PipelineUpdate),
    ProjectOpened,
}

#[derive(Serialize, Deserialize)]
pub struct Editor {
    text_editor: text_editor::TextEditor,
    uniforms_editor: uniforms_editor::UniformsEditor,
}

impl Editor {
    pub fn new() -> (Self, Task<Message>) {
        let (text_editor, text_editor_task) = text_editor::TextEditor::new();
        (
            Self {
                text_editor,
                uniforms_editor: uniforms_editor::UniformsEditor::new(),
            },
            text_editor_task.map(Message::TextEditor),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TextEditor(message) => match message {
                text_editor::Message::UpdatePipeline(shader) => {
                    Task::done(Message::UpdatePipeline(PipelineUpdate::Shader(shader)))
                }
                _ => self.text_editor.update(message).map(Message::TextEditor),
            },
            Message::UniformsEditor(message) => match message {
                uniforms_editor::Message::Update(message) => {
                    Task::done(Message::UpdatePipeline(message))
                }
                _ => self
                    .uniforms_editor
                    .update(message)
                    .map(Message::UniformsEditor),
            },
            Message::UpdatePipeline(_) => Task::none(),
            Message::ProjectOpened => Task::done(Message::UpdatePipeline(PipelineUpdate::Shader(
                self.text_editor.content(),
            )))
            .chain(Task::done(Message::UpdatePipeline(
                PipelineUpdate::Uniforms(crate::pipeline_update::UniformsUpdate::Reset(self.uniforms_editor.uniforms())),
            ))),
        }
    }

    pub fn view(&self) -> Element<Message> {
        container(iced::widget::column![
            self.uniforms_editor.view().map(Message::UniformsEditor),
            self.text_editor.view().map(Message::TextEditor),
        ])
        .into()
    }

    pub fn text(&self) -> &text_editor::TextEditor {
        &self.text_editor
    }

    pub fn subscription(&self) -> Subscription<Message> {
        self.uniforms_editor
            .subscription()
            .map(Message::UniformsEditor)
    }
}
