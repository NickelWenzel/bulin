use crate::editor;
use crate::text_editor;

use iced::widget::{button, text, Button};
use iced::{Border, Color, Element, Length};

use iced_aw::menu::Item;
use iced_aw::style::menu_bar::primary;
use iced_aw::{menu, menu_bar};
use iced_aw::{quad, widgets::InnerBounds};

#[derive(Debug, Clone)]
pub enum Message {
    OpenProject,
    SaveProject,
    Editor(editor::Message),
}

pub fn view() -> Element<'static, Message> {
    menu_bar!((
        main_menu_item("File"),
        menu!((menu_item(
            "Open File",
            Message::Editor(editor::Message::TextEditor(text_editor::Message::OpenFile))
        ))(menu_item(
            "Save File",
            Message::Editor(editor::Message::TextEditor(text_editor::Message::SaveFile))
        ))(separator())(menu_item(
            "Open Project",
            Message::OpenProject
        ))(menu_item("Save Project", Message::SaveProject)))
        .max_width(180.0)
    )(
        main_menu_item("Edit"),
        menu!((menu_item(
            "Undo",
            Message::Editor(editor::Message::TextEditor(text_editor::Message::Undo))
        ))(menu_item(
            "Redo",
            Message::Editor(editor::Message::TextEditor(text_editor::Message::Redo))
        )))
        .max_width(180.0)
    ))
    .draw_path(menu::DrawPath::Backdrop)
    .style(primary)
    .into()
}

fn main_menu_item(item_text: &str) -> Button<'_, Message> {
    button(text(item_text)).style(button::text)
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
