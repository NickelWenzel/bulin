// mod pipeline;
mod pipeline;
mod uniforms;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::shader_update::{ShaderUpdate, UniformsUpdate};
use crate::uniforms_editor::uniform::Uniform;

use iced::wgpu;

use iced::Rectangle;
use iced::mouse;
use iced::widget::shader::{self, Viewport};

const EMPTY_FRAG_WGSL: &str = include_str!("shaders/empty_frag.wgsl");
const VERTEX_SHADER: &str = include_str!("shaders/vertex_shader.wgsl");

pub struct CanvasScene {
    primitive_data: Arc<PrimitiveData>,
    uniforms: Vec<Uniform>,
    // Set on every change and cleared by the pipeline once no edited shader
    // awaits validation, keeping frames coming until its result is applied
    validating: Arc<AtomicBool>,
}

impl CanvasScene {
    pub fn new(shader: String) -> Self {
        Self {
            primitive_data: Arc::new(PrimitiveData::new(shader, UniformRenderData::empty())),
            uniforms: Vec::new(),
            validating: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn update(&mut self, message: ShaderUpdate) {
        match message {
            ShaderUpdate::Shader(shader) => self.update_shader(shader),
            ShaderUpdate::Uniforms(uniforms) => self.update_uniforms(uniforms),
        }

        self.validating.store(true, Ordering::Relaxed);
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

    fn update(
        &self,
        _state: &mut Self::State,
        _event: &iced::Event,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Option<shader::Action<Message>> {
        self.validating
            .load(Ordering::Relaxed)
            .then(shader::Action::request_redraw)
    }

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        Primitive {
            data: self.primitive_data.clone(),
            validating: self.validating.clone(),
        }
    }
}

#[derive(Debug)]
pub struct Primitive {
    data: Arc<PrimitiveData>,
    validating: Arc<AtomicBool>,
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
            "{VERTEX_SHADER}\n\n{}\n\n{}",
            self.uniforms.uniforms_str, self.shader
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
    if data.is_empty() {
        return Vec::new();
    }

    let mut bytes = Vec::new();
    let mut struct_alignment = 1;

    for uniform in data {
        let value = uniform.value;
        let alignment = value.wgsl_alignment();
        struct_alignment = struct_alignment.max(alignment);

        bytes.resize(align_up(bytes.len(), alignment), 0);
        let payload_offset = bytes.len();
        value.write_payload(&mut bytes);

        debug_assert_eq!(bytes.len() - payload_offset, value.wgsl_size());
    }

    bytes.resize(align_up(bytes.len(), struct_alignment), 0);
    bytes
}

fn align_up(offset: usize, alignment: usize) -> usize {
    debug_assert!(alignment.is_power_of_two());
    (offset + alignment - 1) & !(alignment - 1)
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
    type Pipeline = pipeline::Pipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        _viewport: &Viewport,
    ) {
        pipeline.prepare(device, queue, self.data.clone());
        self.validating
            .store(pipeline.is_validating(), Ordering::Relaxed);
    }

    fn draw(&self, pipeline: &Self::Pipeline, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        pipeline.draw(render_pass)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uniforms_editor::uniform::Type;

    fn uniform(value: Type) -> Uniform {
        Uniform {
            value,
            name: String::new(),
        }
    }

    fn read_f32(bytes: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    #[test]
    fn empty_uniforms_have_no_bytes() {
        assert!(to_uniforms_bytes(&[]).is_empty());
    }

    #[test]
    fn col3_has_trailing_struct_padding() {
        let bytes = to_uniforms_bytes(&[uniform(Type::Col3((1.0, 2.0, 3.0)))]);

        assert_eq!(bytes.len(), 16);
        assert_eq!(read_f32(&bytes, 0), 1.0);
        assert_eq!(read_f32(&bytes, 4), 2.0);
        assert_eq!(read_f32(&bytes, 8), 3.0);
        assert_eq!(&bytes[12..16], &[0; 4]);
    }

    #[test]
    fn col4_occupies_exactly_sixteen_bytes() {
        let bytes = to_uniforms_bytes(&[uniform(Type::Col4((1.0, 2.0, 3.0, 4.0)))]);

        assert_eq!(bytes.len(), 16);
        assert_eq!(read_f32(&bytes, 12), 4.0);
    }

    #[test]
    fn col4_is_aligned_after_a_scalar() {
        let bytes = to_uniforms_bytes(&[
            uniform(Type::Float(5.0)),
            uniform(Type::Col4((1.0, 2.0, 3.0, 4.0))),
        ]);

        assert_eq!(bytes.len(), 32);
        assert_eq!(read_f32(&bytes, 0), 5.0);
        assert_eq!(&bytes[4..16], &[0; 12]);
        assert_eq!(read_f32(&bytes, 16), 1.0);
        assert_eq!(read_f32(&bytes, 28), 4.0);
    }

    #[test]
    fn scalar_uses_the_remaining_space_after_col3() {
        let bytes = to_uniforms_bytes(&[
            uniform(Type::Col3((1.0, 2.0, 3.0))),
            uniform(Type::Float(4.0)),
        ]);

        assert_eq!(bytes.len(), 16);
        assert_eq!(read_f32(&bytes, 8), 3.0);
        assert_eq!(read_f32(&bytes, 12), 4.0);
    }

    #[test]
    fn mixed_values_use_member_and_struct_alignment() {
        let bytes = to_uniforms_bytes(&[
            uniform(Type::VecFloat2((1.0, 2.0))),
            uniform(Type::Float(3.0)),
            uniform(Type::VecFloat3((4.0, 5.0, 6.0))),
        ]);

        assert_eq!(bytes.len(), 32);
        assert_eq!(read_f32(&bytes, 0), 1.0);
        assert_eq!(read_f32(&bytes, 4), 2.0);
        assert_eq!(read_f32(&bytes, 8), 3.0);
        assert_eq!(&bytes[12..16], &[0; 4]);
        assert_eq!(read_f32(&bytes, 16), 4.0);
        assert_eq!(read_f32(&bytes, 24), 6.0);
        assert_eq!(&bytes[28..32], &[0; 4]);
    }

    #[test]
    fn every_type_reports_and_writes_its_expected_layout() {
        let cases = [
            (Type::Int(1), 4, 4),
            (Type::Float(1.0), 4, 4),
            (Type::VecFloat2((1.0, 2.0)), 8, 8),
            (Type::VecFloat3((1.0, 2.0, 3.0)), 12, 16),
            (Type::Col3((1.0, 2.0, 3.0)), 12, 16),
            (Type::VecFloat4((1.0, 2.0, 3.0, 4.0)), 16, 16),
            (Type::Col4((1.0, 2.0, 3.0, 4.0)), 16, 16),
            (Type::VecInt2((1, 2)), 8, 8),
            (Type::VecInt3((1, 2, 3)), 12, 16),
            (Type::VecInt4((1, 2, 3, 4)), 16, 16),
        ];

        for (value, expected_size, expected_alignment) in cases {
            let mut payload = Vec::new();
            value.write_payload(&mut payload);

            assert_eq!(value.wgsl_size(), expected_size);
            assert_eq!(value.wgsl_alignment(), expected_alignment);
            assert_eq!(payload.len(), expected_size);
        }
    }
}
