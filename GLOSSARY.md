# Glossary

- **Shader**: the fragment shader the user edits. It defines `fs_main` and reads its inputs
  from `customs`.
- **Uniform**: a named value the user adds and adjusts with sliders, such as a radius or a
  color. Its value also fixes its WGSL type. Names are unique WGSL identifiers.
- **Time**: the built-in uniform, the seconds on the clock. While it is on, it is always the
  first member of Customs, under the name `time`.
- **Customs**: the WGSL struct bulin generates from time and the uniforms, bound as
  `customs` at `@group(0) @binding(0)`.
- **Program**: the shader together with the uniforms and time. The viewer compiles and runs
  it: a fixed vertex stage, then the Customs declaration, then the shader.
- **Project**: the saved file holding a shader, its uniforms and the clock.
