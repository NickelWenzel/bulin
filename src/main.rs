use iced::Font;
use bulin_rust::Application;

fn main() -> iced::Result {
    iced::application(
        "Bulin",
        Application::update,
        Application::view,
    )
    .theme(Application::theme)
    .font(include_bytes!("../fonts/icons.ttf").as_slice())
    .default_font(Font::MONOSPACE)
    .subscription(Application::subscription)
    .run_with(Application::new)
}
