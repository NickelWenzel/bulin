@fragment
fn fs_main(@builtin(position) clip_pos: vec4<f32>) -> @location(0) vec4<f32> {
    let time = 0.1;
    let radius = 0.3;
    let leaves = 7.0;
    let speed = 0.5;
    let flower_color = vec3<f32>(0.9, 0.1, 0.2);
    let test = vec3<f32>(0.1, 0.2, 0.1);

    let st = clip_pos.xy / vec2<f32>(1000.);//uniforms.resolution;

    let d = 0.25 + 0.5 * (0.5 + 0.5 * sin(2.0 * time));
    let pos = vec2<f32>(d) - st;

    let r = (2.0 + sin(9.0 * time)) / radius * length(pos) * 1.2;
    let a = atan2(pos.y, pos.x);

    let f = abs(cos(a * leaves / 2.0 + speed * time)) * 0.5 + 0.3;
    let ratio = smoothstep(f, f + 0.02, r);
    let color = flower_color * (1.0 - ratio);
    let back = test * ratio;

    return vec4<f32>(color + back, 1.0);
}
