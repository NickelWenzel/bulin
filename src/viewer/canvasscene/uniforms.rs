use iced::Rectangle;

#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct DefaultUniforms {
    pub position: [f32; 2],
    pub resolution: [f32; 2],
}
impl DefaultUniforms {
    pub fn new(
        Rectangle {
            x,
            y,
            width,
            height,
        }: Rectangle,
    ) -> Self {
        Self {
            position: [x, y],
            resolution: [width, height],
        }
    }
}

pub type CustomUniforms = [u8];
