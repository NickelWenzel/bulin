use iced::Font;

fn main() -> iced::Result {
    iced::application(
        "Custom Shader - Iced",
        bulin_rust::Application::update,
        bulin_rust::Application::view,
    )
    .theme(bulin_rust::Application::theme)
    .font(include_bytes!("../fonts/icons.ttf").as_slice())
    .default_font(Font::MONOSPACE)
    .subscription(bulin_rust::Application::subscription)
    .run()
}
