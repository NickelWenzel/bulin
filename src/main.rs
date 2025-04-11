use bulin_rust::Application;
use iced::Font;

fn main() -> iced::Result {
    iced::application("Bulin", Application::update, Application::view)
        .subscription(Application::subscription)
        .theme(Application::theme)
        .font(include_bytes!("../fonts/icons.ttf").as_slice())
        .default_font(Font::MONOSPACE)
        .run()
}
