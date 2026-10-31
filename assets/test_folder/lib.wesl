// Edit this file in another editor: the preview should update.

fn rings(distance: f32) -> f32 {
    return 0.5 + 0.5 * sin(distance * 130.0);
}

fn palette(t: f32) -> vec3<f32> {
    return mix(vec3<f32>(0.1, 0.2, 0.6), vec3<f32>(1.0, 0.7, 0.2), t);
}
