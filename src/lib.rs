pub mod background;
pub mod error;
pub mod render;
pub mod ui;

use anyhow::Result;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use crate::background::BackgroundTask;
use crate::error::AppError;
use crate::render::Renderer;

/// Shared application state
pub struct App {
    /// Application name
    pub name: String,
    /// Slider value
    pub value: f32,
    /// Code editor content
    pub code: String,
    /// Background task handle
    pub background_task: Option<BackgroundTask>,
    /// Renderer instance
    pub renderer: Option<Arc<Mutex<Renderer>>>,
    /// Channel for background communication
    pub background_receiver: Option<mpsc::UnboundedReceiver<String>>,
    /// GPU rendered texture
    pub gpu_texture: Option<egui::TextureHandle>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            name: "Bulin GUI App".to_string(),
            value: 0.0,
            code:
                "// Welcome to the code editor!\n\nfn main() {\n    println!(\"Hello, world!\");\n}"
                    .to_string(),
            background_task: None,
            renderer: None,
            background_receiver: None,
            gpu_texture: None,
        }
    }
}

impl App {
    /// Create a new application instance
    pub fn new() -> Self {
        Self::default()
    }

    /// Initialize the application with async components
    pub async fn initialize(&mut self) -> Result<(), AppError> {
        // Initialize renderer
        let renderer = Renderer::new().await?;
        self.renderer = Some(Arc::new(Mutex::new(renderer)));

        // Initialize background task
        let (tx, rx) = mpsc::unbounded_channel();
        self.background_receiver = Some(rx);

        let mut background_task = BackgroundTask::new(tx);
        tokio::spawn(async move {
            if let Err(e) = background_task.start().await {
                eprintln!("Background task error: {e}");
            }
        });

        Ok(())
    }

    /// Update background messages
    fn update_background_messages(&mut self) {
        if let Some(ref mut receiver) = self.background_receiver {
            while let Ok(message) = receiver.try_recv() {
                println!("Background message: {message}");
            }
        }
    }

    /// Render GPU texture
    fn render_gpu_texture(&mut self, ctx: &egui::Context) -> Result<(), AppError> {
        if let Some(ref renderer) = self.renderer {
            let _renderer = renderer.lock().unwrap();
            // TODO: Implement actual GPU rendering
            // For now, create a placeholder texture
            if self.gpu_texture.is_none() {
                let blue_pixel = [0, 0, 255, 255u8]; // RGBA
                let mut pixels = Vec::new();
                for _ in 0..(64 * 64) {
                    pixels.extend_from_slice(&blue_pixel);
                }
                let color_image = egui::ColorImage::from_rgba_unmultiplied([64, 64], &pixels);
                self.gpu_texture =
                    Some(ctx.load_texture("gpu_texture", color_image, Default::default()));
            }
        }
        Ok(())
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Update background messages
        self.update_background_messages();

        // Render GPU texture
        let _ = self.render_gpu_texture(ctx);

        // Main UI
        egui::CentralPanel::default().show(ctx, |ui| {
            self.ui(ui);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_creation() {
        let app = App::new();
        assert_eq!(app.name, "Bulin GUI App");
        assert_eq!(app.value, 0.0);
        assert!(app.code.contains("Hello, world!"));
    }

    #[test]
    fn test_app_default() {
        let app = App::default();
        assert_eq!(app.name, "Bulin GUI App");
        assert_eq!(app.value, 0.0);
    }
}
