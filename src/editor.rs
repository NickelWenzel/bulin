use matcha_iced::lsp::update;

use crate::analyzer::Analyzer;
use crate::shader;
use crate::shader_update::ShaderUpdate;
use crate::uniforms;

use iced::Subscription;
use iced::time::Instant;
use iced::widget::container;
use iced::{Element, Task};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub enum Message {
    TextEditor(shader::Message),
    UniformsEditor(uniforms::Message),
    UpdatePipeline(ShaderUpdate),
    ProjectOpened,
}

#[derive(Serialize, Deserialize)]
pub struct Editor {
    text_editor: shader::State,
    uniforms_editor: uniforms::State,
}

impl Editor {
    pub fn new(shader: &str) -> Self {
        Self {
            text_editor: shader::State::new(shader),
            uniforms_editor: uniforms::State::new(),
        }
    }

    pub fn update(
        &mut self,
        message: Message,
        now: Instant,
        analyzer: &mut Analyzer,
    ) -> Task<Message> {
        match message {
            Message::TextEditor(message) => match message {
                shader::Message::UpdatePipeline(shader) => {
                    Task::done(Message::UpdatePipeline(ShaderUpdate::Shader(shader)))
                }
                _ => self
                    .text_editor
                    .update(message, now, analyzer)
                    .map(Message::TextEditor),
            },
            Message::UniformsEditor(message) => match message {
                uniforms::Message::Update(message) => Task::done(Message::UpdatePipeline(message)),
                _ => self
                    .uniforms_editor
                    .update(message)
                    .map(Message::UniformsEditor),
            },
            Message::ProjectOpened => {
                self.text_editor.open_lsp(analyzer);

                Task::done(Message::UpdatePipeline(ShaderUpdate::Shader(
                    self.text_editor.content(),
                )))
                .chain(Task::done(Message::UpdatePipeline(
                    ShaderUpdate::Uniforms(crate::shader_update::UniformsUpdate::Reset(
                        self.uniforms_editor.uniforms(),
                    )),
                )))
            }
            Message::UpdatePipeline(_) => Task::none(),
        }
    }

    pub fn view(&self, analyzer: &Analyzer) -> Element<'_, Message> {
        container(iced::widget::column![
            self.uniforms_editor.view().map(Message::UniformsEditor),
            self.text_editor.view(analyzer).map(Message::TextEditor),
        ])
        .into()
    }

    /// Opens the shader on the language server.
    pub fn open_lsp(&mut self, analyzer: &mut Analyzer) {
        self.text_editor.open_lsp(analyzer);
    }

    /// Closes the shader on the language server.
    pub fn close_lsp(&mut self, analyzer: &mut Analyzer) {
        self.text_editor.close_lsp(analyzer);
    }

    /// Applies what the language server sent for the shader.
    pub fn apply_lsp(
        &mut self,
        analyzer: &mut Analyzer,
        document: update::Document,
    ) -> Task<Message> {
        self.text_editor
            .apply_lsp(analyzer, document)
            .map(Message::TextEditor)
    }

    pub fn text(&self) -> &shader::State {
        &self.text_editor
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            self.text_editor.subscription().map(Message::TextEditor),
            self.uniforms_editor
                .subscription()
                .map(Message::UniformsEditor),
        ])
    }
}
