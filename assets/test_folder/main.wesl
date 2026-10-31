import package::lib::{palette, rings};

fn mainBulin(pixel: bulin::Pixel) -> vec4<f32> {
    let centered = pixel.uv - vec2<f32>(0.5);
    let time = uniforms::customs.time;
    let t = rings(length(centered) - 0.15 * time);
    return vec4<f32>(palette(t), 1.0);
}
