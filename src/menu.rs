use crate::editor;
use crate::text_editor;

use iced::widget::column;
use iced::widget::container;
use iced::widget::{button, text, Button};
use iced::{Border, Color, Element, Length};

use iced_aw::{quad, widgets::InnerBounds};

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
        separator(),
        menu_item("Open Project", Message::OpenProject),
        menu_item("New Project", Message::NewProject),
        menu_item("Save Project", Message::SaveProject),
        menu_item("Save Project as", Message::SaveProjectAs),
        separator(),
        menu_item(
            "Undo",
            Message::Editor(editor::Message::TextEditor(text_editor::Message::Undo))
        ),
        menu_item(
            "Redo",
            Message::Editor(editor::Message::TextEditor(text_editor::Message::Redo))
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

fn separator() -> quad::Quad {
    quad::Quad {
        quad_color: Color::from([0.5; 3]).into(),
        quad_border: Border::default(),
        inner_bounds: InnerBounds::Ratio(0.995, 0.05),
        height: Length::Fixed(1.),
        ..Default::default()
    }
}
