use iced::widget::text_editor;
use iced::{Length, Theme, highlighter, keyboard};

use crate::shader_update::FragmentShader;
use crate::util::{self, FileName};

use iced::{
    Center, Element, Font, Task,
    widget::{button, column, container, pick_list, row, space, text, toggler, tooltip},
};

use serde::{Deserialize, Serialize};

use std::sync::Arc;

// custom string serialization for content
#[expect(dead_code, reason = "used in custom serde")]
const CONTENT_SERDE: &str = include_str!("content_serde.rs");

// stable id
const EDITOR: &str = "editor";

#[derive(Serialize, Deserialize)]
pub struct TextEditor {
    file: Option<FileName>,
    #[serde(with = "CONTENT_SERDE", skip)]
    content: text_editor::Content,
    #[serde(default = "default_theme", skip)]
    theme: highlighter::Theme,
    #[serde(default = "bool::default", skip)]
    word_wrap: bool,
    #[serde(default = "bool::default", skip)]
    is_loading: bool,
    #[serde(default = "bool::default", skip)]
    is_dirty: bool,
}

fn default_theme() -> highlighter::Theme {
    highlighter::Theme::SolarizedDark
}

#[derive(Debug, Clone)]
pub enum Message {
    ActionPerformed(text_editor::Action),
    ThemeSelected(highlighter::Theme),
    WordWrapToggled(bool),
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
            content: text_editor::Content::with_text(shader),
            theme: default_theme(),
            is_loading: false,
            is_dirty: false,
            word_wrap: false,
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ActionPerformed(action) => {
                let is_dirty = action.is_edit();
                self.content.perform(action);

                if is_dirty {
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
                    self.content = text_editor::Content::new();
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
                    self.content = text_editor::Content::with_text(&contents);
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

    pub fn view(&'_ self) -> Element<'_, Message> {
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
            toggler(self.word_wrap)
                .label("Word Wrap")
                .on_toggle(Message::WordWrapToggled),
            pick_list(
                Some(&self.theme),
                highlighter::Theme::ALL,
                highlighter::Theme::to_string
            )
            .on_select(Message::ThemeSelected)
            .text_size(14)
            .padding([5, 10])
        ]
        .spacing(10)
        .align_y(Center);

        column![
            controls,
            text_editor(&self.content)
                .id(EDITOR)
                .height(Length::Fill)
                .on_action(Message::ActionPerformed)
                .wrapping(if self.word_wrap {
                    text::Wrapping::Word
                } else {
                    text::Wrapping::None
                })
                .highlight("wgsl", self.theme)
                .key_binding(|key_press| {
                    match key_press.key.as_ref() {
                        keyboard::Key::Character("s") if key_press.modifiers.command() => {
                            Some(text_editor::Binding::Custom(Message::SaveFile))
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
        match self.theme {
            highlighter::Theme::SolarizedDark => Theme::SolarizedDark,
            highlighter::Theme::Base16Mocha => Theme::CatppuccinMocha,
            theme => {
                if theme.is_dark() {
                    Theme::Dark
                } else {
                    Theme::Light
                }
            }
        }
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
    const ICON_FONT: Font = Font::new("editor-icons");

    text(codepoint)
        .font(ICON_FONT)
        .shaping(text::Shaping::Basic)
        .into()
}
