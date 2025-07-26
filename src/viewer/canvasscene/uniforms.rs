#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct DefaultUniforms {
    pub resolution: [f32; 2],
}
impl DefaultUniforms {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            resolution: [width, height],
        }
    }
}

impl Default for DefaultUniforms {
    fn default() -> Self {
        Self::new(1000.0, 1000.0) // Default resolution
    }
}

pub type CustomUniforms = [u8];
