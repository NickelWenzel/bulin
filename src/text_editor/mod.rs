use iced::Length;
// mod wgsl_highlighter;
use iced_code_editor::{theme, CodeEditor};
// use wgsl_highlighter::WGSLHighlighter;

use crate::shader_update::FragmentShader;
use crate::util::{self, FileName};

use iced::{
    widget::{button, column, container, pick_list, row, space, text, toggler, tooltip},
    Center, Element, Font, Task, Theme,
};

use serde::{Deserialize, Serialize};

use std::sync::Arc;

#[derive(Serialize, Deserialize)]
pub struct TextEditor {
    file: Option<FileName>,
    #[serde(default = "default_editor", skip)]
    editor: CodeEditor,
    #[serde(default = "default_theme", skip)]
    theme: Theme,
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

fn default_editor() -> CodeEditor {
    CodeEditor::new("", "wgsl")
}

#[derive(Debug, Clone)]
pub enum Message {
    EditorMessage(iced_code_editor::Message),
    ThemeSelected(Theme),
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
            editor: CodeEditor::new(shader, "wgsl"),
            theme: default_theme(),
            is_loading: false,
            is_dirty: false,
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::EditorMessage(msg) => {
                let task = self.editor.update(&msg).map(Message::EditorMessage);

                if self.editor.is_modified() {
                    self.is_dirty = true;
                    Task::batch([task, Task::done(Message::UpdatePipeline(self.content()))])
                } else {
                    task
                }
            }
            Message::ThemeSelected(theme) => {
                self.editor.set_theme(theme::from_iced_theme(&theme));
                self.theme = theme;

                Task::none()
            }
            Message::WordWrapToggled(word_wrap) => {
                self.editor.set_wrap_enabled(word_wrap);

                Task::none()
            }
            Message::NewFile => {
                if !self.is_loading {
                    self.file = None;
                    self.editor = CodeEditor::new("", "wgsl");
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
                    self.editor = CodeEditor::new(&contents, "wgsl");
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
                    self.editor.mark_saved();
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
            toggler(self.editor.wrap_enabled())
                .label("Word Wrap")
                .on_toggle(Message::WordWrapToggled),
            pick_list(Theme::ALL, Some(&self.theme), Message::ThemeSelected)
                .text_size(14)
                .padding([5, 10])
        ]
        .spacing(10)
        .align_y(Center);

        column![
            controls,
            row![self.editor.view().map(Message::EditorMessage)].height(Length::Fill),
            //.height(Fill),
            // highlight_with::<WGSLHighlighter>(
            //     Settings {
            //         theme: self.theme,
            //         token: "wgsl".to_string(),
            //     },
            //     |highlight, _theme| highlight.to_format(),
            // )
            // .key_binding(|key_press| {
            //     match key_press.key.as_ref() {
            //         keyboard::Key::Character("s") if key_press.modifiers.command() => {
            //             Some(text_editor::Binding::Custom(Message::SaveFile))
            //         }
            //         keyboard::Key::Character("z") if key_press.modifiers.command() => {
            //             Some(text_editor::Binding::Custom(Message::EditorMessage(
            //                 text_editor::Action::Undo,
            //             )))
            //         }
            //         keyboard::Key::Character("y") if key_press.modifiers.command() => {
            //             Some(text_editor::Binding::Custom(Message::EditorMessage(
            //                 text_editor::Action::Redo,
            //             )))
            //         }
            //         keyboard::Key::Named(keyboard::key::Named::Delete) => {
            //             Some(text_editor::Binding::Delete)
            //         }
            //         keyboard::Key::Named(keyboard::key::Named::Tab) => {
            //             Some(text_editor::Binding::Custom(Message::EditorMessage(
            //                 text_editor::Action::Edit(text_editor::Edit::Paste(Arc::new(
            //                     String::from("  "),
            //                 ))),
            //             )))
            //         }
            //         _ => text_editor::Binding::from_key_press(key_press),
            //     }
            // }),
        ]
        .spacing(10)
        .padding(10)
        .into()
    }

    pub fn theme(&self) -> &iced::Theme {
        &self.theme
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
        self.editor.content()
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
