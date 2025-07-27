use crate::editor;
use crate::text_editor;

use iced::widget::column;
use iced::widget::container;
use iced::widget::rule;
use iced::widget::text_editor::Action;
use iced::widget::{button, text, Button};
use iced::{Element, Length};

#[derive(Debug, Clone)]
pub enum Message {
    NewProject,
    OpenProject,
    SaveProject,
    SaveProjectAs,
    Editor(editor::Message),
}

pub fn view() -> Element<'static, Message> {
    container(column![
        menu_item(
            "Open File",
            Message::Editor(editor::Message::TextEditor(text_editor::Message::OpenFile))
        ),
        menu_item(
            "New File",
            Message::Editor(editor::Message::TextEditor(text_editor::Message::NewFile))
        ),
        menu_item(
            "Save File",
            Message::Editor(editor::Message::TextEditor(text_editor::Message::SaveFile))
        ),
        menu_item(
            "Save File as",
            Message::Editor(editor::Message::TextEditor(
                text_editor::Message::SaveFileAs
            ))
        ),
        rule::Rule::horizontal(1),
        menu_item("Open Project", Message::OpenProject),
        menu_item("New Project", Message::NewProject),
        menu_item("Save Project", Message::SaveProject),
        menu_item("Save Project as", Message::SaveProjectAs),
        rule::Rule::horizontal(1),
        menu_item(
            "Undo",
            Message::Editor(editor::Message::TextEditor(
                text_editor::Message::ActionPerformed(Action::Undo,)
            )),
        ),
        menu_item(
            "Redo",
            Message::Editor(editor::Message::TextEditor(
                text_editor::Message::ActionPerformed(Action::Redo,)
            )),
        )
    ])
    .width(180.0)
    .padding(10)
    .style(container::rounded_box)
    .into()
}

fn menu_item(item_text: &str, message: Message) -> Button<'_, Message> {
    button(text(item_text))
        .style(button::text)
        .on_press(message)
        .width(Length::Fill)
}
