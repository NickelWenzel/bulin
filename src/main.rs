#![warn(clippy::all)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use bulin::App;
use eframe::egui;

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
    // Set up logging
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_min_inner_size([600.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Bulin Cross-Platform GUI App",
        options,
        Box::new(|_cc| {
            let mut app = App::new();
            // Initialize async components in a blocking context
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    if let Err(e) = app.initialize().await {
                        eprintln!("Failed to initialize app: {e}");
                    }
                });
            });
            Ok(Box::new(app))
        }),
    )
}
