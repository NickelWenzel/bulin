mod pipeline;
mod uniforms;

use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use pipeline::Pipeline;

use iced_wgpu::wgpu;

use iced::mouse;
use iced::widget::shader::{self, Viewport};
use iced::Rectangle;

use crate::shader_update::ShaderUpdate;
use crate::viewer::canvasscene::uniforms::DefaultUniforms;

#[derive(Clone)]
pub struct CanvasScene {
    version: usize,
    shader: Arc<String>,
}

impl CanvasScene {
    pub fn new(shader: String) -> Self {
        Self {
            version: 0,
            shader: Arc::new(shader),
        }
    }

    pub fn update(&mut self, message: ShaderUpdate) {
        self.version += 1;
        match message {
            ShaderUpdate::Shader(shader) => {
                self.shader = Arc::new(shader);
            }
            ShaderUpdate::Uniforms(uniforms_update) => {
                // TODO: Handle uniforms update if necessary
                // This could involve updating the pipeline or uniforms state
                // For now, we just log the update
                println!("Uniforms update received: {uniforms_update:?}");
            }
        }
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
            version: self.version,
            shader: self.shader.clone(),
        }
    }
}

#[derive(Debug)]
pub struct Primitive {
    version: usize,
    shader: Arc<String>,
}

struct PrimitiveVersion(usize);

impl Deref for PrimitiveVersion {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PrimitiveVersion {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl shader::Primitive for Primitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut shader::Storage,
        _bounds: &Rectangle,
        _viewport: &Viewport,
    ) {
        if !storage.has::<Pipeline>() {
            storage.store(PrimitiveVersion(self.version));
            storage.store(Pipeline::new(
                device,
                queue,
                format,
                &self.shader,
                &DefaultUniforms::default(),
            ));
        }

        // Ensure the pipeline is initialized
        if &self.version > storage.get_mut::<PrimitiveVersion>().unwrap() {
            let pipeline = storage.get_mut::<Pipeline>().unwrap();
            pipeline.update_texture(
                device,
                queue,
                format,
                &self.shader,
                &DefaultUniforms::default(),
            );
            storage.store(PrimitiveVersion(self.version));
        }
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &shader::Storage,
        target: &wgpu::TextureView,
        viewport: &Rectangle<u32>,
    ) {
        // At this point our pipeline should always be initialized
        let pipeline = storage.get::<Pipeline>().unwrap();
        pipeline.render(target, encoder, viewport);
    }
}
