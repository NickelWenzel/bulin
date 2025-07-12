pub mod background;
pub mod error;
pub mod render;
pub mod ui;

use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use crate::background::BackgroundTask;
use crate::render::Renderer;

/// Shared application state
pub struct App {
    /// Code editor content
    pub code: String,
    /// Background task handle
    pub background_task: Option<BackgroundTask>,
    /// Renderer instance
    pub renderer: Arc<Mutex<Renderer>>,
    /// Channel for background communication
    pub background_receiver: Option<mpsc::UnboundedReceiver<String>>,
    /// GPU rendered texture
    pub gpu_texture: Option<egui::TextureHandle>,
}

impl App {
    /// Create a new application instance
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        // Initialize renderer using existing wgpu objects
        let renderer = Renderer::new(device, queue, include_str!("../assets/shader.wgsl"));
        let renderer = Arc::new(Mutex::new(renderer));

        // Initialize background task
        let (tx, rx) = mpsc::unbounded_channel();
        let background_receiver = Some(rx);

        // let mut background_task = BackgroundTask::new(tx);
        // tokio::spawn(async move {
        //     if let Err(e) = background_task.start().await {
        //         eprintln!("Background task error: {e}");
        //     }
        // });
        Self {
            code:
                "// Welcome to the code editor!\n\nfn main() {\n    println!(\"Hello, world!\");\n}"
                    .to_string(),
            background_task: None,
            renderer,
            background_receiver,
            gpu_texture: None,
        }
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
    fn render_gpu_texture(&mut self, ctx: &egui::Context) {
        let _renderer = self.renderer.lock().unwrap();
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
