use iced::border::Radius;
use iced::widget::{button, text};
use iced::{Border, Color, Element, Length};

use iced_aw::menu::{self, Item};
use iced_aw::{menu_bar, menu_items};
use iced_aw::{quad, widgets::InnerBounds};

#[derive(Debug, Clone)]
pub enum Message {
    OpenProject,
    SaveProject,
    OpenFile,
    SaveFile,
    Undo,
    Redo,
}

#[rustfmt::skip]
pub fn view() -> Element<'static, Message> {
    let menu_template = |items| menu::Menu::new(items).max_width(180.0).offset(6.0);
    menu_bar!(
        (button(text("File")), menu_template(
            menu_items!(
                (button(text("Open File")).on_press(Message::OpenFile))
                (button(text("Save File")).on_press(Message::SaveFile))
                (separator())
                (button(text("Open Project")).on_press(Message::OpenProject))
                (button(text("Save Project")).on_press(Message::SaveProject))
            )
        ))
        (button(text("Edit")), menu_template(
            menu_items!(
                (button(text("Undo")).on_press(Message::Undo))
                (button(text("Redo")).on_press(Message::Redo))
            )
        ))
    ).into()
}

fn separator() -> quad::Quad {
    quad::Quad {
        quad_color: Color::from([0.5; 3]).into(),
        quad_border: Border {
            radius: Radius::new(4.0),
            ..Default::default()
        },
        inner_bounds: InnerBounds::Ratio(0.98, 0.2),
        height: Length::Fixed(20.0),
        ..Default::default()
    }
}
