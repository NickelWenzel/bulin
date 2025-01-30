#[derive(Debug)]
pub struct Uniforms {
    pub resolution: glam::Vec2,
}
impl Uniforms {
    pub fn to_raw(&self) -> Raw {
        Raw {
            resolution: self.resolution,
        }
    }
}

#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Raw {
    pub resolution: glam::Vec2,
}
