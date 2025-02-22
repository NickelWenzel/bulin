use iced::Rectangle;

#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Uniforms {
    pub position: [f32; 2],
    pub resolution: [f32; 2],
}
impl Uniforms {
    pub fn new(bounds: Rectangle) -> Self {
        Self {
            position: [bounds.x, bounds.y],
            resolution: [bounds.width, bounds.height],
        }
    }
}
