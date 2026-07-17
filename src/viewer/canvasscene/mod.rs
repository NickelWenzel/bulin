// mod pipeline;
mod pipeline2;
mod uniforms;

use std::sync::Arc;

use crate::shader_update::{ShaderUpdate, UniformsUpdate};
use crate::uniforms_editor::uniform::{Type, Uniform};

use iced_wgpu::wgpu;

use iced::Rectangle;
use iced::mouse;
use iced::widget::shader::{self, Viewport};

const EMPTY_FRAG_WGSL: &str = include_str!("shaders/empty_frag.wgsl");
const VERTEX_SHADER: &str = include_str!("shaders/vertex_shader.wgsl");

pub struct CanvasScene {
    primitive_data: Arc<PrimitiveData>,
    uniforms: Vec<Uniform>,
}

impl CanvasScene {
    pub fn new(shader: String) -> Self {
        Self {
            primitive_data: Arc::new(PrimitiveData::new(shader, UniformRenderData::empty())),
            uniforms: Vec::new(),
        }
    }

    pub fn update(&mut self, message: ShaderUpdate) {
        match message {
            ShaderUpdate::Shader(shader) => self.update_shader(shader),
            ShaderUpdate::Uniforms(uniforms) => self.update_uniforms(uniforms),
        }
    }

    fn update_shader(&mut self, shader: String) {
        self.primitive_data = Arc::new(PrimitiveData::new(
            shader,
            self.primitive_data.uniforms.clone(),
        ));
    }

    fn update_uniforms(&mut self, uniforms: UniformsUpdate) {
        match uniforms {
            UniformsUpdate::Add(uniform) => {
                self.uniforms.push(uniform);
                self.primitive_data = Arc::new(PrimitiveData::new(
                    self.primitive_data.shader.clone(),
                    UniformRenderData::from_uniforms(&self.uniforms),
                ));
            }
            UniformsUpdate::Update(name, uniform) => {
                if let Some(item) = self.uniforms.iter_mut().find(|e| e.name == name) {
                    *item = uniform;
                    self.primitive_data = Arc::new(PrimitiveData::new(
                        self.primitive_data.shader.clone(),
                        UniformRenderData::from_uniforms(&self.uniforms),
                    ));
                }
            }
            UniformsUpdate::Remove(name) => {
                if let Some(idx) = self.uniforms.iter().position(|e| e.name == name) {
                    self.uniforms.remove(idx);
                    self.primitive_data = Arc::new(PrimitiveData::new(
                        self.primitive_data.shader.clone(),
                        UniformRenderData::from_uniforms(&self.uniforms),
                    ));
                }
            }
            UniformsUpdate::Clear => {
                self.uniforms.clear();
                self.primitive_data = Arc::new(PrimitiveData::new(
                    self.primitive_data.shader.clone(),
                    UniformRenderData::empty(),
                ));
            }
            UniformsUpdate::Reset(uniforms) => {
                self.primitive_data = Arc::new(PrimitiveData::new(
                    self.primitive_data.shader.clone(),
                    UniformRenderData::from_uniforms(&uniforms),
                ));
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
            data: self.primitive_data.clone(),
        }
    }
}

#[derive(Debug)]
pub struct Primitive {
    data: Arc<PrimitiveData>,
}

#[derive(Debug)]
struct PrimitiveData {
    shader: String,
    uniforms: UniformRenderData,
}

impl PrimitiveData {
    fn new(shader: String, uniforms: UniformRenderData) -> Self {
        Self { shader, uniforms }
    }

    fn empty() -> Self {
        Self {
            shader: String::from(EMPTY_FRAG_WGSL),
            uniforms: UniformRenderData::empty(),
        }
    }

    fn uniforms_size(&self) -> u64 {
        self.uniforms.size()
    }

    fn whole_shader(&self) -> String {
        format!(
            "{}\n{}\n{}",
            VERTEX_SHADER, self.uniforms.uniforms_str, self.shader
        )
    }
}

#[derive(Debug, Clone)]
pub struct UniformRenderData {
    uniforms_str: String,
    uniforms_bytes: Vec<u8>,
}

impl UniformRenderData {
    pub fn empty() -> Self {
        Self {
            uniforms_str: String::new(),
            uniforms_bytes: Vec::new(),
        }
    }

    pub fn from_uniforms(data: &[Uniform]) -> Self {
        Self {
            uniforms_str: to_uniforms_string(data),
            uniforms_bytes: to_uniforms_bytes(data),
        }
    }

    pub fn size(&self) -> u64 {
        self.uniforms_bytes.len() as u64
    }
}

fn to_uniforms_bytes(data: &[Uniform]) -> Vec<u8> {
    let mut bytes = Vec::new();
    // Pre-allocate memory
    bytes.reserve(data.iter().fold(0, |acc, e| {
        acc + match e.value {
            Type::Int(_) | Type::Float(_) => 4,
            Type::VecFloat2(_) | Type::VecInt2(_) => 8,
            Type::VecFloat3(_) | Type::Col3(_) | Type::VecInt3(_) => 12,
            Type::VecFloat4(_) | Type::Col4(_) | Type::VecInt4(_) => 16,
        }
    }));

    // Push bytes
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

@group(0) @binding(0) var<uniform> customs: Customs;"#,
        data.iter()
            .map(Uniform::to_shader_line)
            .collect::<Vec<_>>()
            .join(",\n")
    )
}

impl shader::Primitive for Primitive {
    type Pipeline = pipeline2::Pipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        pipeline.prepare(device, queue, bounds, viewport, self.data.clone())
    }

    fn draw(&self, pipeline: &Self::Pipeline, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        pipeline.draw(render_pass)
    }
}
