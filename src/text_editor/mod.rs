use iced::time::Instant;
use iced::{Subscription, Theme, event, keyboard};
use scrive_core::SyntaxDef;
use scrive_iced::{CodeEditor, Event};

use crate::shader_update::FragmentShader;
use crate::util::{self, FileName};

use iced::{
    Center, Element, Font, Task,
    widget::{button, column, container, row, text, tooltip},
};

use serde::{Deserialize, Serialize};

use std::sync::Arc;

const WGSL_SYNTAX: &str = include_str!("../../assets/WGSL.sublime-syntax");

#[derive(Serialize, Deserialize)]
pub struct TextEditor {
    file: Option<FileName>,
    #[serde(default = "default_editor", skip)]
    editor: CodeEditor,
    #[serde(default = "bool::default", skip)]
    is_loading: bool,
    #[serde(default = "bool::default", skip)]
    is_dirty: bool,
}

fn default_editor() -> CodeEditor {
    wgsl_editor("")
}

fn wgsl_editor(source: &str) -> CodeEditor {
    let wgsl = SyntaxDef::from_sublime_syntax(WGSL_SYNTAX).expect("bundled WGSL grammar parses");

    CodeEditor::new(source)
        .language(wgsl)
        .line_comment(Some("//"))
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
        }
    }

    pub fn update(&mut self, message: Message, now: Instant) -> Task<Message> {
        match message {
            Message::Editor(event) => {
                let task = self.editor.update(event, now).map(Message::Editor);

                if self.editor.take_dirty() {
                    self.is_dirty = true;
                    Task::batch([task, Task::done(Message::UpdatePipeline(self.content()))])
                } else {
                    task
                }
            }
            Message::NewFile => {
                if !self.is_loading {
                    self.file = None;
                    self.editor = wgsl_editor("");
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
                    self.editor = wgsl_editor(&contents);
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

    // matches the bundled Scrive Dark syntax theme
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
