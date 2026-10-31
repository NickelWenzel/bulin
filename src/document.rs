use iced::time::Instant;
use iced::{Subscription, Theme, event, keyboard};
use matcha_core::{Severity, TreeSitterDef};
use matcha_iced::keymap::Keymap;
use matcha_iced::lsp::client;
use matcha_iced::{CodeEditor, Event};

use crate::action::Action;
use crate::analyzer::Analyzer;
use crate::core;
use crate::core::folder::{Path, same_text};
use crate::settings::keymap;
use crate::{file, folder};

use iced::widget::container;
use iced::{Element, Task};

use std::sync::Arc;

pub struct State {
    location: Location,
    /// The file's path, shortened to fit a title.
    label: Option<String>,
    editor: CodeEditor,
    is_loading: bool,
    /// The text last loaded or saved, which storage holds, or `None` when the file is gone.
    base: Option<String>,
    /// Whether the editor's text differs from `base`, kept current after every change to either.
    dirty: bool,
    /// Whether edits and saves are refused, for the files bulin generates.
    read_only: bool,
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

/// Where the document's text comes from and goes when it is saved.
#[derive(Debug, Clone)]
pub enum Location {
    /// Nowhere yet: saving asks where.
    Untitled,
    /// A file picked in a dialog: a native file or a web upload.
    File(file::Name),
    /// A file of the open folder.
    Folder(Path),
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
    /// The document's text changed. A folder file never says so: the preview follows what is
    /// saved.
    Changed,
    /// Save the folder file `path` with `text`, which was `base` when last loaded or saved.
    Save {
        path: Path,
        text: String,
        base: Option<String>,
    },
}

impl State {
    /// An untitled document holding `shader`, edited with `settings`.
    pub fn new(shader: &str, settings: &core::Settings) -> Self {
        Self {
            location: Location::Untitled,
            label: None,
            editor: wgsl_editor(shader, settings),
            is_loading: false,
            base: Some(shader.to_owned()),
            dirty: false,
            read_only: false,
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

    /// The folder file `path`, holding `text` as storage does, edited with `settings`. bulin's own
    /// files under `.bulin/` are read-only, since bulin rewrites them.
    pub fn folder(path: Path, text: &str, settings: &core::Settings) -> Self {
        Self {
            label: Some(shorten(path.as_str())),
            read_only: path.as_str().starts_with(".bulin/"),
            location: Location::Folder(path),
            ..Self::new(text, settings)
        }
    }

    /// Starts analyzing the shader under its location's name.
    pub fn attach(&mut self, analyzer: &mut Analyzer) {
        analyzer.attach(&mut self.editor, &self.location);
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
            self.check_dirty();
            self.changed(Action::none())
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
            Message::Editor(event) if self.read_only && edits(&event) => Action::none(),
            Message::Editor(event) => {
                let action = Action::task(self.editor.update(event, now).map(Message::Editor));
                analyzer.sync(&mut self.editor);

                if self.editor.take_dirty() {
                    self.check_dirty();
                    self.changed(action)
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
                self.location = Location::Untitled;
                self.label = None;
                self.editor = wgsl_editor(core::program::DEFAULT_SOURCE, settings);
                self.attach(analyzer);
                self.base = Some(crate::core::program::DEFAULT_SOURCE.to_owned());
                self.dirty = false;

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
                        self.location = Location::File(path);
                        self.editor = wgsl_editor(&contents, settings);
                        self.attach(analyzer);
                        self.base = Some(contents.as_ref().clone());
                        self.dirty = false;
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
                if self.is_loading || self.read_only {
                    return Action::none();
                }
                self.failure = None;
                let file = match &self.location {
                    Location::Folder(path) => {
                        return Action::instruction(Instruction::Save {
                            path: path.clone(),
                            text: self.content(),
                            base: self.base.clone(),
                        });
                    }
                    Location::File(file) => Some(file.clone()),
                    Location::Untitled => None,
                };
                self.is_loading = true;

                Action::task(Task::perform(
                    file::save_file(file, "shader.wesl", self.content()),
                    Message::FileSaved,
                ))
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
                        let location = Location::File(path);
                        analyzer.saved(&mut self.editor, &self.location, &location);
                        self.label = match &location {
                            Location::File(path) => path.as_str().map(shorten),
                            Location::Untitled | Location::Folder(_) => None,
                        };
                        self.location = location;
                        self.base = Some(self.content());
                        self.check_dirty();
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

    /// Takes in how saving the folder file `path` went, when it is still this document: the saved
    /// text on success, or on a conflict what storage holds instead, so the next save overwrites
    /// it.
    pub fn saved(&mut self, path: &Path, result: &Result<String, folder::Error>) {
        // A read-only document saves nothing, so a write to its file is bulin's and reaches it as
        // a change on disk.
        if !self.is_at(path) || self.read_only {
            return;
        }
        match result {
            Ok(text) => self.base = Some(text.clone()),
            Err(folder::Error::Conflict { disk }) => self.base = disk.clone(),
            Err(_) => return,
        }
        self.check_dirty();
    }

    /// Replaces the text of the folder file `path`, which this document holds without unsaved
    /// changes, with what storage now holds. The replacement can be undone like an edit.
    pub fn reloaded(&mut self, path: &Path, text: &str, analyzer: &mut Analyzer) {
        if !self.is_at(path) {
            return;
        }
        self.editor.load(text, None);
        analyzer.sync(&mut self.editor);
        self.editor.take_dirty();
        self.base = Some(text.to_owned());
        self.check_dirty();
    }

    /// Notes that storage holds `text` for the folder file `path`, `None` when the file is gone,
    /// while the editor keeps its own text.
    pub fn disk_changed(&mut self, path: &Path, text: Option<&str>) {
        if !self.is_at(path) {
            return;
        }
        self.base = text.map(str::to_owned);
        self.check_dirty();
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    /// The text last loaded or saved, `None` when the file is gone.
    pub fn base(&self) -> Option<&str> {
        self.base.as_deref()
    }

    /// Whether the editor holds changes storage doesn't.
    pub fn is_dirty(&self) -> bool {
        self.dirty
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
        if self.dirty {
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

    /// Whether this document is the folder file `path`.
    fn is_at(&self, path: &Path) -> bool {
        matches!(&self.location, Location::Folder(at) if at == path)
    }

    /// Recomputes `dirty` from the editor's text and `base`.
    fn check_dirty(&mut self) {
        self.dirty = self
            .base
            .as_deref()
            .is_none_or(|base| !same_text(base, &self.editor.document().text()));
    }

    /// `action`, telling the parent the text changed unless this is a folder file, whose changes
    /// reach the preview when they are saved.
    fn changed(&self, action: Action<Instruction, Message>) -> Action<Instruction, Message> {
        match self.location {
            Location::Folder(_) => action,
            Location::Untitled | Location::File(_) => action.with_instruction(Instruction::Changed),
        }
    }
}

/// Whether `event` changes the text, which a read-only document refuses.
fn edits(event: &Event) -> bool {
    use matcha_iced::Action;

    match event {
        Event::Editor(action) => match action {
            Action::Type(_)
            | Action::Backspace
            | Action::Delete
            | Action::DeleteWordBack
            | Action::DeleteWordForward
            | Action::Enter
            | Action::Tab
            | Action::Outdent
            | Action::ToggleComment
            | Action::DeleteLine
            | Action::InsertLine { .. }
            | Action::Cut
            | Action::Paste { .. }
            | Action::Undo
            | Action::Redo
            | Action::MoveLine { .. }
            | Action::CopyLine { .. }
            | Action::PopupAccept
            | Action::PopupClickAccept(_)
            | Action::Rename
            | Action::Format
            | Action::InlayInsert { .. } => true,
            Action::Move { .. }
            | Action::PlaceCaret(_)
            | Action::DragSelect { .. }
            | Action::AddCaret(_)
            | Action::AddNextOccurrence
            | Action::RemoveNewestCaret
            | Action::SelectAllOccurrences
            | Action::AddCaretVertical { .. }
            | Action::JumpToBracket
            | Action::ExpandSelection
            | Action::ShrinkSelection
            | Action::Collapse
            | Action::SelectAll
            | Action::ColumnSelect(_)
            | Action::ColumnDrag { .. }
            | Action::ViewportChanged(_)
            | Action::PopupUp
            | Action::PopupDown
            | Action::PopupDismiss
            | Action::TriggerCompletion
            | Action::SnippetTab
            | Action::SnippetTabPrev
            | Action::SnippetCancel
            | Action::SignatureClose
            | Action::NextDiagnostic { .. }
            | Action::GotoDefinition
            | Action::HoverQuery(_)
            | Action::HoverDismiss
            | Action::ToggleFold { .. }
            | Action::FoldAtCarets { .. }
            | Action::Wake(_)
            | Action::InlayHover { .. }
            | Action::InlayJump { .. } => false,
        },
        Event::ReplaceOne | Event::ReplaceAll | Event::SubmitRename => true,
        Event::HighlightSweep
        | Event::OpenFind
        | Event::OpenReplace
        | Event::CloseFind
        | Event::FindQuery(_)
        | Event::ToggleCase
        | Event::ToggleWholeWord
        | Event::ToggleRegex
        | Event::ToggleFindInSelection
        | Event::FindNext
        | Event::FindPrev
        | Event::FindSelectAll
        | Event::ToggleReplace
        | Event::ReplaceText(_)
        | Event::TogglePreserveCase
        | Event::RenameText(_)
        | Event::CycleFocus { .. }
        | Event::PointerDown
        | Event::Focused { .. } => false,
        // `Event` is non-exhaustive, so an event matcha adds later can't be told apart and is
        // refused, in case it edits.
        _ => true,
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
    use crate::core::folder::Path;
    use crate::core::{Program, Settings, Uniforms};
    use crate::file;

    #[test]
    fn a_cancelled_open_keeps_unsaved_changes() {
        let (mut analyzer, _events) = Analyzer::start();
        let mut document = State::new("", &Settings::default());
        document.dirty = true;

        let action = document.update(
            Message::FileOpened(Err(file::Error::DialogClosed)),
            Instant::now(),
            &mut analyzer,
            &Settings::default(),
        );

        assert!(action.instruction.is_none());
        assert!(document.dirty);
        assert_eq!(document.failure(), None);
    }

    #[test]
    fn a_failed_open_is_reported() {
        let (mut analyzer, _events) = Analyzer::start();
        let mut document = State::new("", &Settings::default());
        let error = file::Error::Io(Arc::new(std::io::Error::other("disk on fire")));

        let _ = document.update(
            Message::FileOpened(Err(error)),
            Instant::now(),
            &mut analyzer,
            &Settings::default(),
        );

        assert!(document.failure().is_some());
    }

    #[test]
    fn a_new_file_starts_from_a_shader_that_compiles() {
        let (mut analyzer, _events) = Analyzer::start();
        let mut document = State::new("", &Settings::default());
        document.dirty = true;

        let action = document.update(
            Message::NewFile,
            Instant::now(),
            &mut analyzer,
            &Settings::default(),
        );

        assert!(matches!(action.instruction, Some(Instruction::Changed)));
        assert!(!document.dirty);
        assert!(
            Program::project(document.content(), Uniforms::default(), None)
                .compile()
                .is_ok()
        );
    }

    #[test]
    fn typing_in_a_folder_file_marks_it_dirty_without_changing_the_preview() {
        let (mut analyzer, _events) = Analyzer::start();
        let mut document =
            State::folder(Path::new("main.wesl").unwrap(), "a", &Settings::default());
        let typed = Message::Editor(matcha_iced::Event::Editor(matcha_iced::Action::Type('x')));

        let action = document.update(typed, Instant::now(), &mut analyzer, &Settings::default());
        assert!(action.instruction.is_none());
        assert!(document.is_dirty());
        assert_eq!(document.title(), "main.wesl •");

        let _ = document.update(
            Message::Undo,
            Instant::now(),
            &mut analyzer,
            &Settings::default(),
        );
        assert!(!document.is_dirty());
    }

    #[test]
    fn long_paths_are_shortened_between_characters() {
        let path = format!("/home/{}/document.wesl", "Übungsaufgaben".repeat(5));

        let short = shorten(&path);

        assert!(short.starts_with("..."));
        assert_eq!(short.chars().count(), 43);
        assert!(short.ends_with("document.wesl"));
    }

    #[test]
    fn an_opened_file_is_analyzed_in_place_of_the_previous_shader() {
        let (mut analyzer, mut events) = Analyzer::start();
        let mut document = State::new("fn f() -> i32 { return 1; }", &Settings::default());
        document.attach(&mut analyzer);

        let opened = Ok((
            file::Name::from("/shaders/broken.wgsl"),
            Arc::new("fn f() -> i32 { return 1.0; }".to_owned()),
        ));
        let action = document.update(
            Message::FileOpened(opened),
            Instant::now(),
            &mut analyzer,
            &Settings::default(),
        );
        assert!(matches!(action.instruction, Some(Instruction::Changed)));

        let deadline = Instant::now() + Duration::from_secs(30);
        while document.problems.errors == 0 {
            assert!(Instant::now() < deadline, "no error within 30 s");

            match events.next().now_or_never() {
                Some(Some(event)) => {
                    let _ = document.lsp(&mut analyzer, event);
                }
                Some(None) => panic!("the server stopped"),
                None => thread::sleep(Duration::from_millis(10)),
            }
        }
    }
}
