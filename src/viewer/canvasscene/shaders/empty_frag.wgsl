@fragment
fn fs_main(@builtin(position) clip_pos: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(0.2, 0.3, 0.2, 1.0);
}
