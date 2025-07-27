mod content;
use content::Content;
mod wgsl_highlighter;
use wgsl_highlighter::WGSLHighlighter;

use crate::shader_update::FragmentShader;
use crate::util;

use iced::keyboard;
use iced::widget::{
    button, column, container, horizontal_space, pick_list, row, text, text_editor, toggler,
    tooltip,
};
use iced::{Center, Element, Fill, Font, Task};

use iced_highlighter::{Settings, Theme};

use serde::{Deserialize, Serialize};

use std::path::PathBuf;
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
pub struct TextEditor {
    file: Option<PathBuf>,
    content: Content,
    #[serde(default = "default_theme", skip)]
    theme: Theme,
    word_wrap: bool,
    #[serde(default = "default_false", skip)]
    is_loading: bool,
    #[serde(default = "default_false", skip)]
    is_dirty: bool,
}

fn default_theme() -> Theme {
    Theme::SolarizedDark
}

fn default_false() -> bool {
    false
}

#[derive(Debug, Clone)]
pub enum Message {
    ActionPerformed(text_editor::Action),
    ThemeSelected(Theme),
    WordWrapToggled(bool),
    NewFile,
    OpenFile,
    FileOpened(Result<(PathBuf, Arc<String>), util::Error>),
    SaveFile,
    SaveFileAs,
    FileSaved(Result<PathBuf, util::Error>),
    UpdatePipeline(FragmentShader),
}

impl TextEditor {
    pub fn new(shader: &str) -> Self {
        Self {
            file: None,
            content: Content::with_text(shader),
            theme: default_theme(),
            word_wrap: true,
            is_loading: false,
            is_dirty: false,
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ActionPerformed(action) => {
                let is_edit = action.is_edit();

                self.content.perform(action);

                if is_edit {
                    self.is_dirty = true;
                    Task::done(Message::UpdatePipeline(self.content()))
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
                    self.content = Content::new();
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
                    self.file = Some(path);
                    self.content = Content::with_text(&contents);
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
                    self.file = Some(path);
                    self.is_dirty = false;
                }

                Task::none()
            }
            Message::UpdatePipeline(_) => Task::none(),
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
            pick_list(Theme::ALL, Some(self.theme), Message::ThemeSelected)
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
                .highlight_with::<WGSLHighlighter>(
                    Settings {
                        theme: self.theme,
                        token: "wgsl".to_string(),
                    },
                    |highlight, _theme| highlight.to_format(),
                )
                .key_binding(|key_press| {
                    match key_press.key.as_ref() {
                        keyboard::Key::Character("s") if key_press.modifiers.command() => {
                            Some(text_editor::Binding::Custom(Message::SaveFile))
                        }
                        keyboard::Key::Character("z") if key_press.modifiers.command() => {
                            Some(text_editor::Binding::Custom(Message::ActionPerformed(
                                text_editor::Action::Undo,
                            )))
                        }
                        keyboard::Key::Character("y") if key_press.modifiers.command() => {
                            Some(text_editor::Binding::Custom(Message::ActionPerformed(
                                text_editor::Action::Redo,
                            )))
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

    pub fn theme(&self) -> iced::Theme {
        if self.theme.is_dark() {
            iced::Theme::Dark
        } else {
            iced::Theme::Light
        }
    }

    pub fn filename_display_text(&self) -> Option<String> {
        if let Some(path) = &self.file {
            let mut path = path.display().to_string();

            if path.len() > 60 {
                path = format!("...{}", &path[path.len() - 40..]);
            }

            if self.is_dirty {
                path = format!("{path} •");
            }

            Some(path)
        } else {
            None
        }
    }

    pub fn content(&self) -> String {
        self.content.text()
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
