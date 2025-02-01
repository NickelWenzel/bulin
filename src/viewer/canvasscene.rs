mod pipeline;
mod uniforms;

use crate::FragmentShader;

use pipeline::Pipeline;

use iced_wgpu::wgpu;

use iced::mouse;
use iced::widget::shader::{self, Viewport};
use iced::Rectangle;

use std::sync::Arc;

#[derive(Clone)]
pub struct CanvasScene {
    last_valid_shader: Arc<FragmentShader>,
    version: usize,
}

impl CanvasScene {
    pub fn new() -> Self {
        Self {
            last_valid_shader: Arc::new(
                include_str!("canvasscene/shaders/empty_frag.wgsl").to_string(),
            ),
            version: 0,
        }
    }

    pub fn update(&mut self, shader: Arc<FragmentShader>) {
        self.last_valid_shader = shader;
        self.version += 1;
    }
}

impl<Message> shader::Program<Message> for CanvasScene {
    type State = ();
    type Primitive = Primitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        Primitive {
            shader: self.last_valid_shader.clone(),
            version: self.version,
        }
    }
}

/// A collection of `Cube`s that can be rendered.
#[derive(Debug)]
pub struct Primitive {
    pub shader: Arc<String>,
    version: usize,
}

impl shader::Primitive for Primitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut shader::Storage,
        bounds: &Rectangle,
        _viewport: &Viewport,
    ) {
        let should_store = storage
            .get_mut::<Pipeline>()
            .map(|pipeline| {
                if pipeline.version < self.version {
                    pipeline.version = self.version;
                    true
                } else {
                    false
                }
            })
            .unwrap_or(true);

        if should_store {
            match Pipeline::new(device, format, &self.shader, self.version) {
                Ok(pipeline) => storage.store(pipeline),
                Err(error) => {
                    println!("Failed to create pipeline:\n{error}");
                    return;
                }
            }
        }

        let pipeline = storage.get_mut::<Pipeline>().unwrap();
        pipeline.update(queue, &uniforms::Uniforms::new(*bounds));
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &shader::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        // At this point our pipeline should always be initialized
        let pipeline = storage.get::<Pipeline>().unwrap();

        // Render primitive
        pipeline.render(target, encoder, clip_bounds.clone());
    }
}
