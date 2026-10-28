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
    Find,
    Replace,
    Settings,
}

/// The menu, offering Find and Replace only while the find bar is available.
pub fn view(find: bool) -> Element<'static, Message> {
    container(column![
        menu_item("Open File", Some(Message::OpenFile)),
        menu_item("New File", Some(Message::NewFile)),
        menu_item("Save File", Some(Message::SaveFile)),
        menu_item("Save File as", Some(Message::SaveFileAs)),
        rule::horizontal(1),
        menu_item("Open Project", Some(Message::OpenProject)),
        menu_item("New Project", Some(Message::NewProject)),
        menu_item("Save Project", Some(Message::SaveProject)),
        menu_item("Save Project as", Some(Message::SaveProjectAs)),
        rule::horizontal(1),
        menu_item("Undo", Some(Message::Undo)),
        menu_item("Redo", Some(Message::Redo)),
        menu_item("Find", find.then_some(Message::Find)),
        menu_item("Replace", find.then_some(Message::Replace)),
        rule::horizontal(1),
        menu_item("Settings", Some(Message::Settings)),
    ])
    .width(180.0)
    .padding(10)
    .style(container::rounded_box)
    .into()
}

/// An entry that sends `message`, or is disabled without one.
fn menu_item(item_text: &str, message: Option<Message>) -> Button<'_, Message> {
    button(text(item_text))
        .style(button::text)
        .on_press_maybe(message)
        .width(Length::Fill)
}
