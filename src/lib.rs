pub mod background;
pub mod render;
pub mod ui;
pub mod uniforms;

use eframe::egui_wgpu::RenderState;
use tokio::sync::mpsc;

use crate::background::BackgroundTask;
use crate::render::{Renderer, update_texture};

/// Shared application state
pub struct App {
    /// Code editor content
    pub fragment_shader: String,
    pub texture: wgpu::Texture,
    pub texture_view: wgpu::TextureView,
    /// Background task handle
    pub background_task: Option<BackgroundTask>,
    /// Channel for background communication
    pub background_receiver: Option<mpsc::Receiver<String>>,
}

impl App {
    /// Create a new application instance
    #[must_use]
    pub fn new(wgpu_render_state: &RenderState) -> Self {
        // Initialize renderer using existing wgpu objects

        let fragment_shader = include_str!("shaders/empty_frag.wgsl").to_string();

        wgpu_render_state
            .device
            .on_uncaptured_error(Box::new(|e| eprintln!("WGPU error: {e:?}")));

        let texture = wgpu_render_state
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("Offscreen Render Target"),
                size: wgpu::Extent3d {
                    width: 1000,
                    height: 1000,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu_render_state.target_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        update_texture(
            &wgpu_render_state.device,
            &wgpu_render_state.queue,
            wgpu_render_state.target_format,
            &texture_view,
            &fragment_shader,
        );

        let bulin_renderer = Renderer::new(wgpu_render_state, &texture_view);

        wgpu_render_state
            .renderer
            .write()
            .callback_resources
            .insert(bulin_renderer);

        // Initialize background task
        let (tx, rx) = mpsc::channel(10);
        let background_receiver = Some(rx);

        Self {
            fragment_shader,
            texture,
            texture_view,
            background_task: None,
            background_receiver,
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
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Update background messages
        self.update_background_messages();

        // Main UI
        self.show(ctx, frame);
    }
}
