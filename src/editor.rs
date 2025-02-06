mod highlighter;
mod visitor;
use crate::util;

use iced::keyboard;
use iced::widget::{
    self, button, column, container, horizontal_space, pick_list, row, text, text_editor, toggler,
    tooltip,
};
use iced::{Center, Element, Fill, Font, Task, Theme};

use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::ffi;

pub struct Editor {
    file: Option<PathBuf>,
    content: text_editor::Content,
    theme: highlighter::Theme,
    word_wrap: bool,
    is_loading: bool,
    is_dirty: bool,
    undo_handler: UndoHandler,
}

#[derive(Debug, Clone)]
pub enum Message {
    ActionPerformed(text_editor::Action),
    ThemeSelected(highlighter::Theme),
    WordWrapToggled(bool),
    NewFile,
    OpenFile,
    FileOpened(Result<(PathBuf, Arc<String>), util::Error>),
    SaveFile,
    FileSaved(Result<PathBuf, util::Error>),
    UpdatePipeline(Arc<String>),
    Undo,
    Redo,
}

impl Editor {
    pub fn new() -> (Self, Task<Message>) {
        (
            Editor::simple_new(),
            Task::batch([
                Task::perform(
                    util::load_file(format!(
                        "{}/src/viewer/canvasscene/shaders/empty_frag.wgsl",
                        env!("CARGO_MANIFEST_DIR")
                    )),
                    Message::FileOpened,
                ),
                widget::focus_next(),
            ]),
        )
    }

    pub fn simple_new() -> Self {
        Self {
            file: None,
            content: text_editor::Content::new(),
            theme: highlighter::Theme::SolarizedDark,
            word_wrap: true,
            is_loading: true,
            is_dirty: false,
            undo_handler: UndoHandler::new(),
        }
    }

    pub fn with_file(self, file: Option<PathBuf>) -> Self {
        Self { file, ..self }
    }

    pub fn with_content(self, content: &str) -> Self {
        Self {
            content: text_editor::Content::with_text(content),
            ..self
        }
    }

    pub fn with_theme(self, theme: highlighter::Theme) -> Self {
        Self { theme, ..self }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ActionPerformed(action) => {
                let is_edit = action.is_edit();

                let undo_actions = self.create_undo(&action);
                self.undo_handler.push(undo_actions);

                self.content.perform(action);

                if is_edit {
                    self.is_dirty = true;
                    Task::done(Message::UpdatePipeline(Arc::new(self.content.text())))
                } else {
                    Task::none()
                }
            }
            Message::ThemeSelected(theme) => {
                self.theme = theme;

                Task::none()
            }
            Message::WordWrapToggled(word_wrap) => {
                self.word_wrap = word_wrap;

                Task::none()
            }
            Message::NewFile => {
                if !self.is_loading {
                    self.file = None;
                    self.content = text_editor::Content::new();
                }

                Task::none()
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
                    self.file = Some(path);
                    self.content = text_editor::Content::with_text(&contents);
                }

                Task::done(Message::UpdatePipeline(Arc::new(self.content.text())))
            }
            Message::SaveFile => {
                if self.is_loading {
                    Task::none()
                } else {
                    self.is_loading = true;

                    Task::perform(
                        util::save_file(self.file.clone(), self.content.text()),
                        Message::FileSaved,
                    )
                }
            }
            Message::FileSaved(result) => {
                self.is_loading = false;

                if let Ok(path) = result {
                    self.file = Some(path);
                    self.is_dirty = false;
                }

                Task::none()
            }
            Message::UpdatePipeline(_) => Task::none(),
            Message::Undo => {
                self.undo_handler.undo().into_iter().for_each(|action| {
                    self.content.perform(action);
                });

                Task::done(Message::UpdatePipeline(Arc::new(self.content.text())))
            }
            Message::Redo => {
                self.undo_handler.redo().into_iter().for_each(|action| {
                    self.content.perform(action);
                });

                Task::done(Message::UpdatePipeline(Arc::new(self.content.text())))
            }
        }
    }

    pub fn view(&self) -> Element<Message> {
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
            horizontal_space(),
            toggler(self.word_wrap)
                .label("Word Wrap")
                .on_toggle(Message::WordWrapToggled),
            pick_list(
                highlighter::Theme::ALL,
                Some(self.theme),
                Message::ThemeSelected
            )
            .text_size(14)
            .padding([5, 10])
        ]
        .spacing(10)
        .align_y(Center);

        column![
            controls,
            text_editor(&self.content)
                .height(Fill)
                .on_action(Message::ActionPerformed)
                .wrapping(if self.word_wrap {
                    text::Wrapping::Word
                } else {
                    text::Wrapping::None
                })
                .highlight_with::<highlighter::Highlighter>(
                    highlighter::Settings {
                        theme: self.theme,
                        token: self
                            .file
                            .as_deref()
                            .and_then(Path::extension)
                            .and_then(ffi::OsStr::to_str)
                            .unwrap_or("wgsl")
                            .to_owned(),
                    },
                    |highlight, _theme| highlight.to_format(),
                )
                .key_binding(|key_press| {
                    match key_press.key.as_ref() {
                        keyboard::Key::Character("s") if key_press.modifiers.command() => {
                            Some(text_editor::Binding::Custom(Message::SaveFile))
                        }
                        keyboard::Key::Character("z") if key_press.modifiers.command() => {
                            Some(text_editor::Binding::Custom(Message::Undo))
                        }
                        keyboard::Key::Character("y") if key_press.modifiers.command() => {
                            Some(text_editor::Binding::Custom(Message::Redo))
                        }
                        keyboard::Key::Named(keyboard::key::Named::Delete) => {
                            Some(text_editor::Binding::Delete)
                        }
                        keyboard::Key::Named(keyboard::key::Named::Tab) => {
                            Some(text_editor::Binding::Custom(Message::ActionPerformed(
                                text_editor::Action::Edit(text_editor::Edit::Paste(Arc::new(
                                    String::from("  "),
                                ))),
                            )))
                        }
                        _ => text_editor::Binding::from_key_press(key_press),
                    }
                }),
        ]
        .spacing(10)
        .padding(10)
        .into()
    }

    pub fn theme(&self) -> Theme {
        if self.theme.is_dark() {
            Theme::Dark
        } else {
            Theme::Light
        }
    }

    pub fn file_text(&self) -> String {
        if let Some(path) = &self.file {
            let path = path.display().to_string();

            if path.len() > 60 {
                format!("...{}", &path[path.len() - 40..])
            } else {
                path
            }
        } else {
            String::from("New file")
        }
    }

    fn create_undo(&mut self, action: &text_editor::Action) -> Vec<text_editor::Action> {
        let mut ret = Vec::new();
        match action {
            text_editor::Action::Edit(edit) => {
                let selection = self.content.selection().unwrap_or_default();
                let has_selection = !selection.is_empty();
                ret.push(text_editor::Action::Edit(text_editor::Edit::Paste(
                    Arc::new(selection),
                )));

                let mut insert_add_offset_from_cursor = |offset: isize| {
                    let (line, column) = self.content.cursor_position();
                    if let Some(line) = self.content.line(line) {
                        if let Some(char) = line.chars().nth((column as isize + offset) as usize) {
                            ret.push(text_editor::Action::Edit(text_editor::Edit::Insert(char)));
                        }
                    }
                };

                match edit {
                    text_editor::Edit::Enter | text_editor::Edit::Insert(_) => {
                        ret.push(text_editor::Action::Edit(text_editor::Edit::Delete));
                        ret.push(text_editor::Action::Select(text_editor::Motion::Left));
                    }
                    text_editor::Edit::Paste(str) => {
                        if !str.is_empty() {
                            ret.push(text_editor::Action::Edit(text_editor::Edit::Delete));
                            str.chars().into_iter().for_each(|_| {
                                ret.push(text_editor::Action::Select(text_editor::Motion::Left))
                            });
                        }
                    }
                    text_editor::Edit::Backspace if !has_selection => {
                        insert_add_offset_from_cursor(-1)
                    }
                    text_editor::Edit::Delete if !has_selection => insert_add_offset_from_cursor(0),
                    _ => (),
                }
            }
            _ => (),
        }
        ret
    }
}

impl Serialize for Editor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let content = self.content.text();

        let mut state = serializer.serialize_struct("Editor", 3)?;
        state.serialize_field("file", &self.file)?;
        state.serialize_field("content", &content)?;
        state.serialize_field("theme", &self.theme)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Editor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_struct("Editor", &["file", "content", "theme"], visitor::EditorVisitor)
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
    const ICON_FONT: Font = Font::with_name("editor-icons");

    text(codepoint).font(ICON_FONT).into()
}

struct UndoHandler {
    undo_actions: Vec<text_editor::Action>,
    actions_per_undo: Vec<usize>,
    redo_actions: Vec<text_editor::Action>,
    actions_per_redo: Vec<usize>,
}

impl UndoHandler {
    fn new() -> Self {
        Self {
            undo_actions: Vec::new(),
            actions_per_undo: Vec::new(),
            redo_actions: Vec::new(),
            actions_per_redo: Vec::new(),
        }
    }

    fn push(&mut self, actions: Vec<text_editor::Action>) {
        self.actions_per_undo.push(actions.len());
        self.undo_actions.extend(actions);
        self.redo_actions.clear();
        self.actions_per_redo.clear();
    }

    fn undo(&mut self) -> Vec<text_editor::Action> {
        match self
            .actions_per_undo
            .pop()
            .inspect(|n| self.actions_per_redo.push(*n))
        {
            None => Vec::new(),
            Some(action_count) => Self::transfer_and_return_tail(
                &mut self.undo_actions,
                &mut self.redo_actions,
                action_count,
            ),
        }
    }

    fn redo(&mut self) -> Vec<text_editor::Action> {
        match self
            .actions_per_redo
            .pop()
            .inspect(|n| self.actions_per_undo.push(*n))
        {
            None => Vec::new(),
            Some(action_count) => Self::transfer_and_return_tail(
                &mut self.redo_actions,
                &mut self.undo_actions,
                action_count,
            ),
        }
    }

    fn transfer_and_return_tail(
        take_from: &mut Vec<text_editor::Action>,
        add_to: &mut Vec<text_editor::Action>,
        last_n: usize,
    ) -> Vec<text_editor::Action> {
        let actions: Vec<_> = take_from.drain(take_from.len() - last_n..).rev().collect();
        add_to.extend(actions.iter().map(|a| a.clone()));
        actions
    }
}

impl Default for UndoHandler {
    fn default() -> Self {
        Self {
            undo_actions: Vec::new(),
            actions_per_undo: Vec::new(),
            redo_actions: Vec::new(),
            actions_per_redo: Vec::new(),
        }
    }
}
