/// The pixel `mainBulin` draws.
struct Pixel {
    /// Normalized coordinates, from 0 to 1 across the canvas.
    uv: vec2<f32>,
    /// The position in the framebuffer.
    position: vec4<f32>,
}
