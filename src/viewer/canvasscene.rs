mod pipeline;
mod uniforms;

use std::ops::Deref;
use std::sync::Arc;

use crate::pipeline_update::{PipelineUpdate, TimeUpdate, UniformsUpdate};
use crate::uniforms_editor::uniform::Uniform;

use iced::futures::executor::block_on;
use pipeline::Pipeline;

use iced_wgpu::wgpu;

use iced::mouse;
use iced::widget::shader::{self, Viewport};
use iced::Rectangle;

#[derive(Clone)]
pub struct CanvasScene {
    shader: VersionedShader,
    uniforms: VersionedUniforms,
}

impl CanvasScene {
    pub fn new() -> Self {
        Self {
            shader: VersionedShader {
                data: Arc::new(include_str!("canvasscene/shaders/empty_frag.wgsl").to_string()),
                version: 0,
            },
            uniforms: VersionedUniforms {
                data: Vec::new(),
                version: 0,
            },
        }
    }

    pub fn update(&mut self, message: PipelineUpdate) {
        match message {
            PipelineUpdate::Shader(shader) => self.update_shader(shader),
            PipelineUpdate::Uniforms(uniforms) => self.update_uniforms(uniforms),
            PipelineUpdate::Time(time) => self.update_time(time),
        }
    }

    fn update_shader(&mut self, shader: String) {
        self.shader.data = Arc::new(shader);
        self.shader.version += 1;
    }

    fn update_uniforms(&mut self, uniforms: UniformsUpdate) {
        match uniforms {
            UniformsUpdate::Add(uniform) => {
                self.uniforms.data.push(uniform);
                self.uniforms.version += 1;
            }
            UniformsUpdate::Update(idx, uniform) => {
                if let Some(item) = self.uniforms.data.get_mut(idx as usize) {
                    *item = uniform;
                }
            }
            UniformsUpdate::Remove(idx) => {
                self.uniforms.data.remove(idx as usize);
                self.uniforms.version += 1;
            }
            UniformsUpdate::Clear => {
                self.uniforms.data.clear();
                self.uniforms.version += 1;
            }
        }
    }

    fn update_time(&mut self, time: TimeUpdate) {
        match time {
            TimeUpdate::Add => todo!(),
            TimeUpdate::Remove => todo!(),
            TimeUpdate::Tick(instant) => todo!(),
        }
        self.uniforms.version += 1;
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
        Primitive::new(self.shader.clone(), &self.uniforms)
    }
}

#[derive(Debug, Clone)]
pub struct VersionedData<T> {
    pub data: T,
    pub version: usize,
}

type VersionedShader = VersionedData<Arc<String>>;
type VersionedUniforms = VersionedData<Vec<Uniform>>;

#[derive(Debug)]
pub struct Primitive {
    shader: VersionedShader,
    uniforms: VersionedUniformRenderData,
}

impl Primitive {
    pub fn new(shader: VersionedShader, uniforms: &VersionedUniforms) -> Self {
        Self {
            shader,
            uniforms: VersionedUniformRenderData {
                data: UniformRenderData {
                    uniforms_str: to_uniforms_string(&uniforms.data),
                    uniforms_bytes: to_uniforms_bytes(&uniforms.data),
                    uniforms_size: uniforms.data.len(),

                },
                version: uniforms.version,
            }
        }
    }
}

#[derive(Debug)]
pub struct UniformRenderData {
    uniforms_str: String,
    uniforms_bytes: Vec<u8>,
    uniforms_size: usize,
}

type VersionedUniformRenderData = VersionedData<UniformRenderData>;

fn to_uniforms_bytes(data: &Vec<Uniform>) -> Vec<u8> {
    todo!()
}

fn to_uniforms_string(data: &Vec<Uniform>) -> String {
    todo!()
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
        if !storage.has::<Pipeline>() {
            storage.store(Pipeline::new(device));
        }
        let Some(pipeline) = storage.get_mut::<Pipeline>() else {
            return println!("Failed to create pipeline:\n");
        };

        device.push_error_scope(wgpu::ErrorFilter::Validation);

        pipeline
            .update(device, format, &self.shader, &self.uniforms)
            .update_default_buffer(queue, &uniforms::DefaultUniforms::new(bounds.clone()))
            .update_custom_buffer(queue, &self.uniforms.data.uniforms_bytes);

        if let Some(error) = block_on(device.pop_error_scope()) {
            println!("Failed to create pipeline:\n{error}");
        }
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &shader::Storage,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        // At this point our pipeline should always be initialized
        if let Some(pipeline) = storage.get::<Pipeline>() {
            // Render primitive
            pipeline.render(target, encoder, *clip_bounds);
        }
    }
}
