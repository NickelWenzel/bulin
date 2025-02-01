struct Uniforms {
    position: vec2<f32>,
    resolution: vec2<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;