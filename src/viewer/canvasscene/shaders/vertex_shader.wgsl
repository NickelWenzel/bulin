@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2f(vec2u((index << 1) & 2, index & 2));
    return vec4f(uv * 2. - 1., 0., 1.);
    // let uv = vec2<f32>(vec2u((index << 1) & 2, index & 2));
    // let pos = vec4<f32>(uv * 2. - 1., 0., 1.);

    // var transform: mat4x4<f32> = mat4x4<f32>(
    //     vec4<f32>(uniforms.scale.x, 0.0, 0.0, 0.0),
    //     vec4<f32>(0.0, uniforms.scale.y, 0.0, 0.0),
    //     vec4<f32>(0.0, 0.0, 1.0, 0.0),
    //     vec4<f32>(uniforms.position, 0.0, 1.0),
    // );

    // return uniforms.transform * transform * pos;
}
