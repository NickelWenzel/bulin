use iced::widget::{Button, button, column, container, rule, text};
use iced::{Element, Length};

#[derive(Debug, Clone)]
pub enum Message {
    OpenFile,
    NewFile,
    SaveFile,
    SaveFileAs,
    OpenProject,
    NewProject,
    SaveProject,
    SaveProjectAs,
    Undo,
    Redo,
}

pub fn view() -> Element<'static, Message> {
    container(column![
        menu_item("Open File", Message::OpenFile),
        menu_item("New File", Message::NewFile),
        menu_item("Save File", Message::SaveFile),
        menu_item("Save File as", Message::SaveFileAs),
        rule::horizontal(1),
        menu_item("Open Project", Message::OpenProject),
        menu_item("New Project", Message::NewProject),
        menu_item("Save Project", Message::SaveProject),
        menu_item("Save Project as", Message::SaveProjectAs),
        rule::horizontal(1),
        menu_item("Undo", Message::Undo),
        menu_item("Redo", Message::Redo),
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
