mod pipeline;
mod uniforms;

use std::sync::{Arc, RwLock};

use crate::pipeline_update::{PipelineUpdate, UniformsUpdate};
use crate::uniforms_editor::uniform::{Type, Uniform};

use pipeline::Pipeline;

use iced_wgpu::wgpu;

use iced::mouse;
use iced::widget::shader::{self, Viewport};
use iced::Rectangle;

#[derive(Clone)]
pub struct CanvasScene {
    shader: VersionedShader,
    uniforms: Vec<Uniform>,
    uniforms_render_data: VersionedUniformRenderData,
}

impl CanvasScene {
    pub fn new() -> Self {
        Self {
            shader: VersionedShader {
                data: Arc::new(include_str!("canvasscene/shaders/empty_frag.wgsl").to_string()),
                version: 0,
            },
            uniforms: Vec::new(),
            uniforms_render_data: VersionedUniformRenderData {
                data: UniformRenderData {
                    uniforms_str: Arc::new(String::new()),
                    uniforms_bytes: Arc::new(RwLock::new(Vec::new())),
                    uniforms_size: 0,
                },
                version: 0,
            },
        }
    }

    pub fn update(&mut self, message: PipelineUpdate) {
        match message {
            PipelineUpdate::Shader(shader) => self.update_shader(shader),
            PipelineUpdate::Uniforms(uniforms) => self.update_uniforms(uniforms),
        }
    }

    fn update_shader(&mut self, shader: String) {
        self.shader.data = Arc::new(shader);
        self.shader.version += 1;
    }

    fn update_uniforms(&mut self, uniforms: UniformsUpdate) {
        match uniforms {
            UniformsUpdate::Add(uniform) => {
                self.uniforms.push(uniform);
                let uniforms = self.uniforms.drain(..).collect::<Vec<Uniform>>();
                self.update_uniforms_impl(uniforms.as_slice());
            }
            UniformsUpdate::Update(name, uniform) => {
                if let (Some(item), Ok(mut uniform_bytes)) = (
                    self.uniforms.iter_mut().find(|e| e.name == name),
                    self.uniforms_render_data.data.uniforms_bytes.try_write(),
                ) {
                    *item = uniform;
                    *uniform_bytes = to_uniforms_bytes(&self.uniforms);
                }
            }
            UniformsUpdate::Remove(name) => {
                if let Some(idx) = self.uniforms.iter().position(|e| e.name == name) {
                    self.uniforms.remove(idx);
                    let uniforms = self.uniforms.drain(..).collect::<Vec<Uniform>>();
                    self.update_uniforms_impl(uniforms.as_slice());
                }
            }
            UniformsUpdate::Clear => {
                self.uniforms.clear();
                let uniforms = self.uniforms.drain(..).collect::<Vec<Uniform>>();
                self.update_uniforms_impl(uniforms.as_slice());
            }
            UniformsUpdate::Reset(uniforms) => {
                self.update_uniforms_impl(uniforms.as_slice());
            }
        }
    }

    fn update_uniforms_impl(&mut self, uniforms: &[Uniform]) {
        if let Ok(mut uniform_bytes) = self.uniforms_render_data.data.uniforms_bytes.try_write() {
            self.uniforms = uniforms.to_vec();
            self.uniforms_render_data.data.uniforms_str =
                Arc::new(to_uniforms_string(&self.uniforms));
            *uniform_bytes = to_uniforms_bytes(uniforms);
            self.uniforms_render_data.data.uniforms_size = self.uniforms.len();
            self.uniforms_render_data.version += 1;
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
        Primitive::new(self.shader.clone(), self.uniforms_render_data.clone())
    }
}

#[derive(Debug, Clone)]
pub struct VersionedData<T> {
    pub data: T,
    pub version: usize,
}

type VersionedShader = VersionedData<Arc<String>>;

#[derive(Debug)]
pub struct Primitive {
    shader: VersionedShader,
    uniforms: VersionedUniformRenderData,
}

impl Primitive {
    pub fn new(shader: VersionedShader, uniforms: VersionedUniformRenderData) -> Self {
        Self { shader, uniforms }
    }
}

#[derive(Debug, Clone)]
pub struct UniformRenderData {
    uniforms_str: Arc<String>,
    uniforms_bytes: Arc<RwLock<Vec<u8>>>,
    uniforms_size: usize,
}

type VersionedUniformRenderData = VersionedData<UniformRenderData>;

fn to_uniforms_bytes(data: &[Uniform]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.reserve(data.iter().fold(0, |acc, e| {
        acc + match e.value {
            Type::Int(_) | Type::Float(_) => 4,
            Type::VecFloat2(_) | Type::VecInt2(_) => 8,
            Type::VecFloat3(_) | Type::Col3(_) | Type::VecInt3(_) => 12,
            Type::VecFloat4(_) | Type::Col4(_) | Type::VecInt4(_) => 16,
        }
    }));

    for uniform in data {
        match uniform.value {
            Type::Int(value) => bytes.extend_from_slice(&value.to_ne_bytes()),
            Type::Float(value) => bytes.extend_from_slice(&value.to_ne_bytes()),
            Type::VecFloat2(value) => {
                bytes.extend_from_slice(&value.0.to_ne_bytes());
                bytes.extend_from_slice(&value.1.to_ne_bytes());
            }
            Type::VecFloat3(value) | Type::Col3(value) => {
                bytes.extend_from_slice(&value.0.to_ne_bytes());
                bytes.extend_from_slice(&value.1.to_ne_bytes());
                bytes.extend_from_slice(&value.2.to_ne_bytes());
            }
            Type::VecFloat4(value) | Type::Col4(value) => {
                bytes.extend_from_slice(&value.0.to_ne_bytes());
                bytes.extend_from_slice(&value.1.to_ne_bytes());
                bytes.extend_from_slice(&value.2.to_ne_bytes());
                bytes.extend_from_slice(&value.3.to_ne_bytes());
            }
            Type::VecInt2(value) => {
                bytes.extend_from_slice(&value.0.to_ne_bytes());
                bytes.extend_from_slice(&value.1.to_ne_bytes());
            }
            Type::VecInt3(value) => {
                bytes.extend_from_slice(&value.0.to_ne_bytes());
                bytes.extend_from_slice(&value.1.to_ne_bytes());
                bytes.extend_from_slice(&value.2.to_ne_bytes());
            }
            Type::VecInt4(value) => {
                bytes.extend_from_slice(&value.0.to_ne_bytes());
                bytes.extend_from_slice(&value.1.to_ne_bytes());
                bytes.extend_from_slice(&value.2.to_ne_bytes());
                bytes.extend_from_slice(&value.3.to_ne_bytes());
            }
        }
    }

    bytes
}

fn to_uniforms_string(data: &[Uniform]) -> String {
    if data.is_empty() {
        return String::new();
    }

    format!(
        r#"
struct Customs {{
    {},
}}

@group(1) @binding(0) var<uniform> customs: Customs;"#,
        data.iter()
            .map(|u| u.to_shader_line())
            .collect::<Vec<_>>()
            .join(",\n")
    )
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

        if let Ok(pipeline) = storage
            .get_mut::<Pipeline>()
            .ok_or(String::new())
            .and_then(|pipeline| pipeline.update(device, format, &self.shader, &self.uniforms))
            .inspect_err(|e| error!("Failed to create pipeline:\n{}", e))
        {
            pipeline
                .update_default_buffer(queue, &uniforms::DefaultUniforms::new(bounds.clone()))
                .update_custom_buffer(queue, &self.uniforms.data.uniforms_bytes.read().unwrap());
        };
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
