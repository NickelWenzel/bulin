use matcha_iced::lsp::update;

use crate::analyzer::Analyzer;
use crate::core::{Program, Project};
use crate::shader;
use crate::uniforms;

use iced::Subscription;
use iced::time::Instant;
use iced::widget::container;
use iced::{Element, Task};

#[derive(Debug, Clone)]
pub enum Message {
    TextEditor(shader::Message),
    UniformsEditor(uniforms::Message),
    /// The shader or its uniforms changed.
    Changed,
    ProjectOpened,
}

pub struct Editor {
    text_editor: shader::State,
    uniforms_editor: uniforms::State,
}

impl Editor {
    pub fn new(project: Project) -> Self {
        Self {
            text_editor: shader::State::new(&project.shader),
            uniforms_editor: uniforms::State::new(project.uniforms, project.clock),
        }
    }

    /// The project to save for the current shader and uniforms.
    pub fn project(&self) -> Project {
        Project::new(
            self.text_editor.content(),
            self.uniforms_editor.uniforms().clone(),
            self.uniforms_editor.clock(),
        )
    }

    pub fn update(
        &mut self,
        message: Message,
        now: Instant,
        analyzer: &mut Analyzer,
    ) -> Task<Message> {
        match message {
            Message::TextEditor(message) => match message {
                shader::Message::Changed => Task::done(Message::Changed),
                _ => self
                    .text_editor
                    .update(message, now, analyzer)
                    .map(Message::TextEditor),
            },
            Message::UniformsEditor(message) => match message {
                uniforms::Message::Changed => Task::done(Message::Changed),
                _ => self
                    .uniforms_editor
                    .update(message)
                    .map(Message::UniformsEditor),
            },
            Message::ProjectOpened => {
                self.text_editor.open_lsp(analyzer);
                Task::done(Message::Changed)
            }
            Message::Changed => Task::none(),
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

    /// The program the viewer runs for the current shader and uniforms.
    pub fn program(&self) -> Program {
        Program::new(
            self.text_editor.content(),
            self.uniforms_editor.uniforms().clone(),
            self.uniforms_editor.time(),
        )
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
