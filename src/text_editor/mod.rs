use iced::time::Instant;
use iced::{Subscription, Theme, event, keyboard};
use matcha_core::{Severity, TreeSitterDef};
use matcha_iced::lsp::{client, update};
use matcha_iced::{CodeEditor, Event};

use crate::analyzer::Analyzer;
use crate::shader_update::FragmentShader;
use crate::util::{self, FileName};

use iced::{
    Center, Element, Font, Task,
    widget::{button, column, container, row, space, text, tooltip},
};

use serde::{Deserialize, Serialize};

use std::sync::Arc;

#[derive(Serialize, Deserialize)]
pub struct TextEditor {
    file: Option<FileName>,
    #[serde(default = "default_editor", skip)]
    editor: CodeEditor,
    #[serde(default = "bool::default", skip)]
    is_loading: bool,
    #[serde(default = "bool::default", skip)]
    is_dirty: bool,
    #[serde(skip)]
    problems: Problems,
}

/// What the language server found wrong with the shader.
#[derive(Default)]
struct Problems {
    errors: usize,
    warnings: usize,
}

fn default_editor() -> CodeEditor {
    wgsl_editor("")
}

fn wgsl_editor(source: &str) -> CodeEditor {
    let wesl = TreeSitterDef::new(
        tree_sitter_wesl::LANGUAGE,
        tree_sitter_wesl::HIGHLIGHTS_QUERY,
    )
    .expect("tree-sitter-wesl's highlights query compiles");

    CodeEditor::new(source)
        .language(wesl)
        .line_comment(Some("//"))
        .inlay_hints(true)
        .rename(true)
}

#[derive(Debug, Clone)]
pub enum Message {
    Editor(Event),
    NewFile,
    OpenFile,
    FileOpened(Result<(FileName, Arc<String>), util::Error>),
    SaveFile,
    SaveFileAs,
    FileSaved(Result<FileName, util::Error>),
    UpdatePipeline(FragmentShader),
}

impl TextEditor {
    pub fn new(shader: &str) -> Self {
        Self {
            file: None,
            editor: wgsl_editor(shader),
            is_loading: false,
            is_dirty: false,
            problems: Problems::default(),
        }
    }

    /// Opens the shader on the language server under its file's name.
    pub fn open_lsp(&mut self, analyzer: &mut Analyzer) {
        let uri = analyzer.document_uri(self.file.as_ref());
        if let Err(error) = self.editor.open_lsp(analyzer.client(), &uri, "wgsl") {
            tracing::warn!("cannot open {uri} on wgsl-analyzer: {error}");
        }
    }

    /// Closes the shader on the language server, clearing what it reported.
    pub fn close_lsp(&mut self, analyzer: &mut Analyzer) {
        self.editor.close_lsp(analyzer.client());
        self.problems = Problems::default();
    }

    /// Applies what the language server sent for the shader. Its edits, such as formatting or a
    /// rename, update the pipeline like typing does.
    pub fn apply_lsp(
        &mut self,
        analyzer: &mut Analyzer,
        document: update::Document,
    ) -> Task<Message> {
        log_refusal(self.editor.apply_lsp(analyzer.client(), document));

        let diagnostics = self.editor.document().diagnostics_in(0..u32::MAX);
        self.problems = diagnostics.fold(Problems::default(), |mut problems, (_, severity, _)| {
            match severity {
                Severity::Error => problems.errors += 1,
                Severity::Warning => problems.warnings += 1,
                Severity::Info | Severity::Hint => {}
            }
            problems
        });

        if self.editor.take_dirty() {
            self.is_dirty = true;
            Task::done(Message::UpdatePipeline(self.content()))
        } else {
            Task::none()
        }
    }

    pub fn update(
        &mut self,
        message: Message,
        now: Instant,
        analyzer: &mut Analyzer,
    ) -> Task<Message> {
        match message {
            Message::Editor(event) => {
                let task = self.editor.update(event, now).map(Message::Editor);
                log_refusal(self.editor.sync_lsp(analyzer.client()));

                if self.editor.take_dirty() {
                    self.is_dirty = true;
                    Task::batch([task, Task::done(Message::UpdatePipeline(self.content()))])
                } else {
                    task
                }
            }
            Message::NewFile => {
                if !self.is_loading {
                    self.close_lsp(analyzer);
                    self.file = None;
                    self.editor = wgsl_editor("");
                    self.open_lsp(analyzer);
                }

                Task::done(Message::UpdatePipeline(self.content()))
            }
            Message::OpenFile => {
                if self.is_loading {
                    Task::none()
                } else {
                    self.is_loading = true;

                    Task::perform(util::open_file(), Message::FileOpened)
                }
            }
            Message::FileOpened(result) => {
                self.is_loading = false;
                self.is_dirty = false;

                if let Ok((path, contents)) = result {
                    self.close_lsp(analyzer);
                    self.file = Some(path);
                    self.editor = wgsl_editor(&contents);
                    self.open_lsp(analyzer);
                }

                Task::done(Message::UpdatePipeline(self.content()))
            }
            Message::SaveFile => {
                if self.is_loading {
                    Task::none()
                } else {
                    self.is_loading = true;

                    Task::perform(
                        util::save_file(self.file.clone(), self.content()),
                        Message::FileSaved,
                    )
                }
            }
            Message::SaveFileAs => {
                if self.is_loading {
                    Task::none()
                } else {
                    self.is_loading = true;

                    Task::perform(util::save_file(None, self.content()), Message::FileSaved)
                }
            }
            Message::FileSaved(result) => {
                self.is_loading = false;

                if let Ok(path) = result {
                    let renamed = analyzer.document_uri(self.file.as_ref())
                        != analyzer.document_uri(Some(&path));
                    if renamed {
                        self.close_lsp(analyzer);
                        self.file = Some(path);
                        self.open_lsp(analyzer);
                    } else {
                        self.file = Some(path);
                        log_refusal(self.editor.save_lsp(analyzer.client()));
                    }
                    self.is_dirty = false;
                }

                Task::none()
            }
            Message::UpdatePipeline(_) => Task::none(),
        }
    }

    pub fn view(&self, analyzer: &Analyzer) -> Element<'_, Message> {
        let status = match analyzer.status() {
            client::Status::Starting | client::Status::Restarting { .. } => {
                "wgsl-analyzer: starting…".to_owned()
            }
            client::Status::Running => match self.problems {
                Problems {
                    errors: 0,
                    warnings: 0,
                } => "no problems".to_owned(),
                Problems { errors, warnings } => format!(
                    "{errors} {}, {warnings} {}",
                    if errors == 1 { "error" } else { "errors" },
                    if warnings == 1 { "warning" } else { "warnings" },
                ),
            },
            client::Status::Stopped(_) => match analyzer.failure() {
                Some(failure) => format!("wgsl-analyzer: unavailable, {failure}"),
                None => "wgsl-analyzer: unavailable".to_owned(),
            },
        };

        let controls = row![
            action(new_icon(), "New file", Some(Message::NewFile)),
            action(
                open_icon(),
                "Open file",
                (!self.is_loading).then_some(Message::OpenFile)
            ),
            action(
                save_icon(),
                "Save file",
                self.is_dirty.then_some(Message::SaveFile)
            ),
            space::horizontal(),
            text(status).size(14),
        ]
        .spacing(10)
        .align_y(Center);

        column![controls, self.editor.view().map(Message::Editor)]
            .spacing(10)
            .padding(10)
            .into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            self.editor.subscription().map(Message::Editor),
            event::listen_with(|event, _status, _window| match event {
                iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
                    if modifiers.command() && key.as_ref() == keyboard::Key::Character("s") =>
                {
                    Some(Message::SaveFile)
                }
                _ => None,
            }),
        ])
    }

    // matches the bundled Matcha Dark syntax theme
    pub fn theme(&self) -> Theme {
        Theme::Dark
    }

    pub fn filename_display_text(&self) -> Option<String> {
        let mut path = String::from(self.file.as_ref()?.as_str()?);

        if path.len() > 60 {
            path = format!("...{}", &path[path.len() - 40..])
        }
        if self.is_dirty {
            path = format!("{path} •")
        }
        Some(path)
    }

    pub fn content(&self) -> String {
        self.editor.document().text().into_owned()
    }
}

/// Logs an update the editor refused. Its jump, a definition in another document, is dropped:
/// the editor holds one shader.
fn log_refusal(applied: update::Applied) {
    if let Some(refusal) = applied.refused {
        tracing::debug!("wgsl-analyzer update not applied: {refusal}");
    }
}

fn action<'a, Message: Clone + 'a>(
    content: impl Into<Element<'a, Message>>,
    label: &'a str,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let action = button(container(content).center_x(30));

    if let Some(on_press) = on_press {
        tooltip(
            action.on_press(on_press),
            label,
            tooltip::Position::FollowCursor,
        )
        .style(container::rounded_box)
        .into()
    } else {
        action.style(button::secondary).into()
    }
}

fn new_icon<'a, Message>() -> Element<'a, Message> {
    icon('\u{0e800}')
}

fn save_icon<'a, Message>() -> Element<'a, Message> {
    icon('\u{0e801}')
}

fn open_icon<'a, Message>() -> Element<'a, Message> {
    icon('\u{0f115}')
}

fn icon<'a, Message>(codepoint: char) -> Element<'a, Message> {
    const ICON_FONT: Font = Font::new("editor-icons");

    text(codepoint)
        .font(ICON_FONT)
        .shaping(text::Shaping::Basic)
        .into()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use iced::futures::{FutureExt, StreamExt};
    use iced::time::Instant;
    use matcha_iced::lsp::Update;

    use super::{Message, TextEditor};
    use crate::analyzer::Analyzer;
    use crate::util::FileName;

    #[test]
    fn an_opened_file_is_analyzed_in_place_of_the_previous_shader() {
        let (mut analyzer, mut events) = Analyzer::start();
        let mut editor = TextEditor::new("fn f() -> i32 { return 1; }");
        editor.open_lsp(&mut analyzer);

        let opened = Ok((
            FileName::from("/shaders/broken.wgsl"),
            Arc::new("fn f() -> i32 { return 1.0; }".to_owned()),
        ));
        let _ = editor.update(Message::FileOpened(opened), Instant::now(), &mut analyzer);

        let deadline = Instant::now() + Duration::from_secs(30);
        while editor.problems.errors == 0 {
            assert!(Instant::now() < deadline, "no error within 30 s");
            analyzer.flush();

            match events.next().now_or_never() {
                Some(Some(event)) => {
                    for update in analyzer.client().receive(event) {
                        if let Update::Document(document) = update {
                            let _ = editor.apply_lsp(&mut analyzer, document);
                        }
                    }
                }
                Some(None) => panic!("the server stopped"),
                None => thread::sleep(Duration::from_millis(10)),
            }
        }
    }
}
