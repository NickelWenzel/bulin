use bulin::Application;

fn main() -> iced::Result {
    #[cfg(target_arch = "wasm32")]
    {
        console_log::init().expect("Initialize logger");
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    }

    #[cfg(not(target_arch = "wasm32"))]
    tracing_subscriber::fmt::init();

    iced::application::timed(
        Application::new,
        Application::update,
        Application::subscription,
        Application::view,
    )
    // A close request first asks about unsaved work and waits for the folder's writes.
    .exit_on_close_request(false)
    .theme(Application::theme)
    .fonts([include_bytes!("../fonts/menu.ttf").as_slice()])
    .fonts(matcha_iced::required_fonts().iter().copied())
    .run()
}
