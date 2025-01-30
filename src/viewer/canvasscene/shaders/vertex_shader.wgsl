@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
	let uv = vec2f(vec2u((index << 1) & 2, index & 2));
	return vec4f(uv * 2. - 1., 0., 1.);
}
