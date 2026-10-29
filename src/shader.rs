use iced::time::Instant;
use iced::{Subscription, Theme, event, keyboard};
use matcha_core::{Severity, TreeSitterDef};
use matcha_iced::keymap::Keymap;
use matcha_iced::lsp::client;
use matcha_iced::{CodeEditor, Event};

use crate::action::Action;
use crate::analyzer::Analyzer;
use crate::core;
use crate::file;
use crate::settings::keymap;

use iced::widget::container;
use iced::{Element, Task};

use std::sync::Arc;

pub struct State {
    file: Option<file::Name>,
    /// The file's path, shortened to fit a title.
    label: Option<String>,
    editor: CodeEditor,
    is_loading: bool,
    is_dirty: bool,
    problems: Problems,
    /// Why the last file action failed, until the next one starts.
    failure: Option<String>,
}

/// What the language server found wrong with the shader.
#[derive(Default)]
struct Problems {
    errors: usize,
    warnings: usize,
}

fn wgsl_editor(source: &str, settings: &core::Settings) -> CodeEditor {
    let wesl = TreeSitterDef::new(
        tree_sitter_wesl::LANGUAGE,
        tree_sitter_wesl::HIGHLIGHTS_QUERY,
    )
    .expect("tree-sitter-wesl's highlights query compiles");

    CodeEditor::new(source)
        .language(wesl)
        .line_comment(Some("//"))
        .rename(true)
        .inlay_hints(settings.editor.inlay_hints)
        .show_whitespace(settings.editor.render_whitespace)
        .text_size(settings.editor.font_size)
        .find(settings.editor.find)
        .keymap(bindings(&settings.keybindings))
}

/// The editor's keymap for `keybindings`.
fn bindings(keybindings: &core::settings::Keybindings) -> Keymap {
    keymap::overrides(keybindings)
        .into_iter()
        .fold(Keymap::new(), |keymap, (chord, action)| match action {
            Some(action) => keymap.bind(chord, action),
            None => keymap.unbind(chord),
        })
}

#[derive(Debug, Clone)]
pub enum Message {
    Editor(Event),
    NewFile,
    OpenFile,
    FileOpened(Result<(file::Name, Arc<String>), file::Error>),
    SaveFile,
    SaveFileAs,
    FileSaved(Result<file::Name, file::Error>),
    Undo,
    Redo,
}

pub enum Instruction {
    /// The shader text changed.
    Changed,
}

impl State {
    pub fn new(shader: &str, settings: &core::Settings) -> Self {
        Self {
            file: None,
            label: None,
            editor: wgsl_editor(shader, settings),
            is_loading: false,
            is_dirty: false,
            problems: Problems::default(),
            failure: None,
        }
    }

    /// Makes the editor follow `settings`, keeping its document.
    pub fn apply(&mut self, settings: &core::Settings, now: Instant) -> Task<Message> {
        // Turning find off leaves an open bar drawn, but closing it works either way.
        let close = if settings.editor.find {
            Task::none()
        } else {
            self.editor
                .update(Event::CloseFind, now)
                .map(Message::Editor)
        };

        self.editor.set_inlay_hints(settings.editor.inlay_hints);
        self.editor
            .set_show_whitespace(settings.editor.render_whitespace);
        self.editor.set_keymap(bindings(&settings.keybindings));
        // The text size and find have no setters; their builders only assign a field.
        self.editor = std::mem::replace(&mut self.editor, CodeEditor::new(""))
            .text_size(settings.editor.font_size)
            .find(settings.editor.find);

        close
    }

    /// Starts analyzing the shader under its file's name.
    pub fn attach(&mut self, analyzer: &mut Analyzer) {
        analyzer.attach(&mut self.editor, self.file.as_ref());
    }

    /// Stops analyzing the shader, clearing what the analyzer reported.
    pub fn detach(&mut self, analyzer: &mut Analyzer) {
        analyzer.detach(&mut self.editor);
        self.problems = Problems::default();
    }

    /// Applies an analyzer event. Its edits, such as formatting or a rename, change the shader
    /// like typing does.
    pub fn lsp(
        &mut self,
        analyzer: &mut Analyzer,
        event: client::Event,
    ) -> Action<Instruction, Message> {
        analyzer.receive(event, &mut self.editor);

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
            Action::instruction(Instruction::Changed)
        } else {
            Action::none()
        }
    }

    pub fn update(
        &mut self,
        message: Message,
        now: Instant,
        analyzer: &mut Analyzer,
        settings: &core::Settings,
    ) -> Action<Instruction, Message> {
        match message {
            Message::Editor(event) => {
                let action = Action::task(self.editor.update(event, now).map(Message::Editor));
                analyzer.sync(&mut self.editor);

                if self.editor.take_dirty() {
                    self.is_dirty = true;
                    action.with_instruction(Instruction::Changed)
                } else {
                    action
                }
            }
            Message::Undo => self.update(
                Message::Editor(Event::Editor(matcha_iced::Action::Undo)),
                now,
                analyzer,
                settings,
            ),
            Message::Redo => self.update(
                Message::Editor(Event::Editor(matcha_iced::Action::Redo)),
                now,
                analyzer,
                settings,
            ),
            Message::NewFile => {
                if self.is_loading {
                    return Action::none();
                }
                self.failure = None;
                self.detach(analyzer);
                self.file = None;
                self.label = None;
                self.editor = wgsl_editor(core::program::DEFAULT_SOURCE, settings);
                self.attach(analyzer);
                self.is_dirty = false;

                Action::instruction(Instruction::Changed)
            }
            Message::OpenFile => {
                if self.is_loading {
                    Action::none()
                } else {
                    self.is_loading = true;
                    self.failure = None;

                    Action::task(Task::perform(file::open_file(), Message::FileOpened))
                }
            }
            Message::FileOpened(result) => {
                self.is_loading = false;

                match result {
                    Ok((path, contents)) => {
                        self.detach(analyzer);
                        self.label = path.as_str().map(shorten);
                        self.file = Some(path);
                        self.editor = wgsl_editor(&contents, settings);
                        self.attach(analyzer);
                        self.is_dirty = false;
                        Action::instruction(Instruction::Changed)
                    }
                    Err(file::Error::DialogClosed) => Action::none(),
                    Err(error) => {
                        self.failure = Some(format!("Could not open the shader: {error}"));
                        Action::none()
                    }
                }
            }
            Message::SaveFile => {
                if self.is_loading {
                    Action::none()
                } else {
                    self.is_loading = true;
                    self.failure = None;

                    Action::task(Task::perform(
                        file::save_file(self.file.clone(), "shader.wesl", self.content()),
                        Message::FileSaved,
                    ))
                }
            }
            Message::SaveFileAs => {
                if self.is_loading {
                    Action::none()
                } else {
                    self.is_loading = true;
                    self.failure = None;

                    Action::task(Task::perform(
                        file::save_file(None, "shader.wesl", self.content()),
                        Message::FileSaved,
                    ))
                }
            }
            Message::FileSaved(result) => {
                self.is_loading = false;

                match result {
                    Ok(path) => {
                        analyzer.saved(&mut self.editor, self.file.as_ref(), &path);
                        self.label = path.as_str().map(shorten);
                        self.file = Some(path);
                        self.is_dirty = false;
                    }
                    Err(file::Error::DialogClosed) => {}
                    Err(error) => {
                        self.failure = Some(format!("Could not save the shader: {error}"));
                    }
                }

                Action::none()
            }
        }
    }

    /// What the analyzer says about the shader: its error and warning counts while it runs.
    pub fn status(&self, analyzer: &Analyzer) -> String {
        match analyzer.status() {
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
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        container(self.editor.view().map(Message::Editor))
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

    /// The shader's file for a title, marked when it has unsaved changes.
    pub fn title(&self) -> String {
        let label = self.label.as_deref().unwrap_or("New file");
        if self.is_dirty {
            format!("{label} •")
        } else {
            label.to_owned()
        }
    }

    /// Why the last file action failed.
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    /// Whether a file dialog is open.
    pub fn is_loading(&self) -> bool {
        self.is_loading
    }

    pub fn content(&self) -> String {
        self.editor.document().text().into_owned()
    }
}

/// `path` cut to its last 40 characters when it is longer than 60.
fn shorten(path: &str) -> String {
    let length = path.chars().count();
    if length > 60 {
        let tail: String = path.chars().skip(length - 40).collect();
        format!("...{tail}")
    } else {
        path.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use iced::futures::{FutureExt, StreamExt};
    use iced::time::Instant;

    use super::{Instruction, Message, State, shorten};
    use crate::analyzer::Analyzer;
    use crate::core::{Program, Settings, Uniforms};
    use crate::file;

    #[test]
    fn a_cancelled_open_keeps_unsaved_changes() {
        let (mut analyzer, _events) = Analyzer::start();
        let mut shader = State::new("", &Settings::default());
        shader.is_dirty = true;

        let action = shader.update(
            Message::FileOpened(Err(file::Error::DialogClosed)),
            Instant::now(),
            &mut analyzer,
            &Settings::default(),
        );

        assert!(action.instruction.is_none());
        assert!(shader.is_dirty);
        assert_eq!(shader.failure(), None);
    }

    #[test]
    fn a_failed_open_is_reported() {
        let (mut analyzer, _events) = Analyzer::start();
        let mut shader = State::new("", &Settings::default());
        let error = file::Error::Io(Arc::new(std::io::Error::other("disk on fire")));

        let _ = shader.update(
            Message::FileOpened(Err(error)),
            Instant::now(),
            &mut analyzer,
            &Settings::default(),
        );

        assert!(shader.failure().is_some());
    }

    #[test]
    fn a_new_file_starts_from_a_shader_that_compiles() {
        let (mut analyzer, _events) = Analyzer::start();
        let mut shader = State::new("", &Settings::default());
        shader.is_dirty = true;

        let action = shader.update(
            Message::NewFile,
            Instant::now(),
            &mut analyzer,
            &Settings::default(),
        );

        assert!(matches!(action.instruction, Some(Instruction::Changed)));
        assert!(!shader.is_dirty);
        assert!(
            Program::project(shader.content(), Uniforms::default(), None)
                .compile()
                .is_ok()
        );
    }

    #[test]
    fn long_paths_are_shortened_between_characters() {
        let path = format!("/home/{}/shader.wesl", "Übungsaufgaben".repeat(5));

        let short = shorten(&path);

        assert!(short.starts_with("..."));
        assert_eq!(short.chars().count(), 43);
        assert!(short.ends_with("shader.wesl"));
    }

    #[test]
    fn an_opened_file_is_analyzed_in_place_of_the_previous_shader() {
        let (mut analyzer, mut events) = Analyzer::start();
        let mut editor = State::new("fn f() -> i32 { return 1; }", &Settings::default());
        editor.attach(&mut analyzer);

        let opened = Ok((
            file::Name::from("/shaders/broken.wgsl"),
            Arc::new("fn f() -> i32 { return 1.0; }".to_owned()),
        ));
        let action = editor.update(
            Message::FileOpened(opened),
            Instant::now(),
            &mut analyzer,
            &Settings::default(),
        );
        assert!(matches!(action.instruction, Some(Instruction::Changed)));

        let deadline = Instant::now() + Duration::from_secs(30);
        while editor.problems.errors == 0 {
            assert!(Instant::now() < deadline, "no error within 30 s");

            match events.next().now_or_never() {
                Some(Some(event)) => {
                    let _ = editor.lsp(&mut analyzer, event);
                }
                Some(None) => panic!("the server stopped"),
                None => thread::sleep(Duration::from_millis(10)),
            }
        }
    }
}
