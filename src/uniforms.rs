#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct DefaultUniforms {
    pub resolution: [f32; 2],
}

pub type CustomUniforms = [u8];
