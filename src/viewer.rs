//! The shader preview: renders the edited shader with its uniforms.

mod pipeline;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use iced::widget::shader;
use iced::widget::shader::Viewport;
use iced::{Element, Fill, Rectangle, mouse, wgpu};

use crate::core::Program;

pub struct State {
    compiled: Arc<Compiled>,
    /// The uniforms package of the program `compiled` came from.
    package: String,
    /// Whether the last loaded program has the layout of `compiled`, so its values can be shown
    /// with it even when it doesn't compile.
    layout_matches: bool,
    // Set on every change and cleared by the pipeline once no edited shader
    // awaits validation, keeping frames coming until its result is applied
    validating: Arc<AtomicBool>,
    /// Why the last loaded program doesn't run, in one line, until one does.
    error: Option<String>,
}

impl State {
    pub fn new(program: &Program) -> Self {
        let (compiled, package, error) = match compile(program) {
            Ok(compiled) => (compiled, program.uniforms_package(), None),
            Err(error) => (
                Compiled::default(),
                Program::default().uniforms_package(),
                Some(error),
            ),
        };

        Self {
            layout_matches: package == program.uniforms_package(),
            compiled: Arc::new(compiled),
            package,
            validating: Arc::new(AtomicBool::new(true)),
            error,
        }
    }

    /// Shows `program` from the next frame on. A program that doesn't compile leaves the last
    /// one showing, with `program`'s values when its layout is the same.
    pub fn load(&mut self, program: &Program) {
        let package = program.uniforms_package();
        match compile(program) {
            Ok(compiled) => {
                self.compiled = Arc::new(compiled);
                self.package = package;
                self.layout_matches = true;
                self.error = None;
                self.validating.store(true, Ordering::Relaxed);
            }
            Err(error) => {
                tracing::debug!("the shader doesn't compile: {error}");
                self.error = Some(error);
                self.layout_matches = package == self.package;
                self.set_values(program.bytes());
            }
        }
    }

    /// Shows new uniform values with the current program, without compiling. Ignored while the
    /// last loaded program's layout differs from the one showing.
    pub fn set_values(&mut self, uniforms: Vec<u8>) {
        if !self.layout_matches || self.compiled.uniforms == uniforms {
            return;
        }
        self.compiled = Arc::new(Compiled {
            wgsl: Arc::clone(&self.compiled.wgsl),
            uniforms,
        });
        self.validating.store(true, Ordering::Relaxed);
    }

    /// Why the shader doesn't run, in one line.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn view<Message: 'static>(&self) -> Element<'_, Message> {
        shader(self).width(Fill).height(Fill).into()
    }
}

impl<Message> shader::Program<Message> for State {
    type State = ();
    type Primitive = Primitive;

    fn update(
        &self,
        _state: &mut Self::State,
        _event: &iced::Event,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Option<shader::Action<Message>> {
        self.validating
            .load(Ordering::Relaxed)
            .then(shader::Action::request_redraw)
    }

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        Primitive {
            compiled: self.compiled.clone(),
            validating: self.validating.clone(),
        }
    }
}

#[derive(Debug)]
pub struct Primitive {
    compiled: Arc<Compiled>,
    validating: Arc<AtomicBool>,
}

/// What the GPU needs from a program, computed once per change instead of every frame.
#[derive(Debug)]
struct Compiled {
    wgsl: Arc<str>,
    uniforms: Vec<u8>,
}

/// Compiles `program` and checks the result as the GPU will, so a program the device would reject
/// is reported instead of silently not showing. Errors are one line, in the shader's own names.
fn compile(program: &Program) -> Result<Compiled, String> {
    let wgsl = program.compile().map_err(|error| {
        let error = error.to_string();
        let line = error
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default();
        line.strip_prefix("error: ").unwrap_or(line).to_owned()
    })?;

    // iced's `webgl` feature is what makes wgpu re-export naga on wasm.
    let module =
        wgpu::naga::front::wgsl::parse_str(&wgsl).map_err(|error| unmangle(error.message()))?;
    // A device created without optional features validates with the default capabilities.
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::default(),
    )
    .validate(&module)
    .map_err(|error| {
        let error = error.into_inner();
        let mut cause: &dyn std::error::Error = &error;
        while let Some(source) = cause.source() {
            cause = source;
        }
        unmangle(&cause.to_string())
    })?;

    Ok(Compiled {
        wgsl: wgsl.into(),
        uniforms: program.bytes(),
    })
}

/// Turns wesl's mangled names in `message` back into the names the shader uses.
fn unmangle(message: &str) -> String {
    message
        .replace("package_shader_", "")
        .replace("uniforms_", "uniforms::")
        .replace("bulin_", "bulin::")
}

impl Default for Compiled {
    /// The default program, which always compiles.
    fn default() -> Self {
        let program = Program::default();
        Self {
            wgsl: program
                .compile()
                .expect("invariant: the default program compiles")
                .into(),
            uniforms: program.bytes(),
        }
    }
}

impl shader::Primitive for Primitive {
    type Pipeline = pipeline::Pipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        _viewport: &Viewport,
    ) {
        pipeline.prepare(device, queue, self.compiled.clone());
        self.validating
            .store(pipeline.is_validating(), Ordering::Relaxed);
    }

    fn draw(&self, pipeline: &Self::Pipeline, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        pipeline.draw(render_pass)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::State;
    use crate::core::uniform::Type;
    use crate::core::{Program, Project, Uniforms};

    const READS_RADIUS: &str = "import uniforms::customs;\n\
        fn mainBulin(pixel: bulin::Pixel) -> vec4<f32> {\n\
            return vec4<f32>(customs.radius);\n\
        }\n";

    fn radius(value: f32) -> Uniforms {
        let mut uniforms = Uniforms::default();
        uniforms.add("radius", Type::Float(value)).unwrap();
        uniforms
    }

    #[test]
    fn the_default_and_sample_programs_run() {
        super::compile(&Program::default()).unwrap();

        for json in [
            include_str!("../assets/test_project.bulin"),
            include_str!("../assets/test_project_2.bulin"),
        ] {
            let project = Project::from_json(json).unwrap();
            let program = Program::new(
                project.shader,
                project.uniforms,
                project.clock.map(|clock| clock.seconds()),
            );
            super::compile(&program).unwrap();
        }
    }

    #[test]
    fn a_broken_shader_is_reported_until_one_compiles() {
        let mut viewer = State::new(&Program::default());
        let broken = Program::new("fn mainBulin(", Uniforms::default(), None);

        viewer.load(&broken);
        let error = viewer.error().expect("a broken shader is reported");
        assert!(error.contains("unexpected end of file"), "{error}");
        assert!(!error.starts_with("error:"), "{error}");

        viewer.load(&Program::default());
        assert_eq!(viewer.error(), None);
    }

    #[test]
    fn new_values_keep_the_compiled_program() {
        let mut viewer = State::new(&Program::new(READS_RADIUS, radius(1.0), None));
        let wgsl = Arc::clone(&viewer.compiled.wgsl);

        viewer.set_values(crate::core::program::bytes(None, &radius(2.0)));

        assert!(Arc::ptr_eq(&wgsl, &viewer.compiled.wgsl));
        assert_eq!(viewer.compiled.uniforms, 2.0f32.to_le_bytes());
    }

    #[test]
    fn values_still_show_while_the_shader_is_broken() {
        let mut viewer = State::new(&Program::new(READS_RADIUS, radius(1.0), None));

        viewer.load(&Program::new("fn mainBulin(", radius(1.0), None));
        viewer.set_values(crate::core::program::bytes(None, &radius(2.0)));

        assert!(viewer.error().is_some());
        assert_eq!(viewer.compiled.uniforms, 2.0f32.to_le_bytes());
    }

    #[test]
    fn values_of_another_layout_wait_for_a_compiling_shader() {
        let mut viewer = State::new(&Program::new(READS_RADIUS, radius(1.0), None));

        viewer.load(&Program::new("fn mainBulin(", radius(1.0), Some(5.0)));
        viewer.set_values(crate::core::program::bytes(Some(6.0), &radius(2.0)));

        assert_eq!(viewer.compiled.uniforms, 1.0f32.to_le_bytes());
    }

    #[test]
    fn fixing_the_shader_clears_the_error() {
        let mut viewer = State::new(&Program::new(READS_RADIUS, radius(1.0), None));

        viewer.load(&Program::new("fn mainBulin(", radius(1.0), None));
        viewer.load(&Program::new(READS_RADIUS, radius(1.0), None));

        assert_eq!(viewer.error(), None);
    }

    #[test]
    fn errors_only_the_gpu_would_catch_are_reported() {
        let mut viewer = State::new(&Program::new(READS_RADIUS, radius(1.0), None));
        let good = Arc::clone(&viewer.compiled.wgsl);

        for source in [
            READS_RADIUS.replace("customs.radius", "customs.nope"),
            READS_RADIUS.replace("vec4<f32>(customs.radius)", "1.0"),
        ] {
            viewer.load(&Program::new(source, radius(1.0), None));

            let error = viewer.error().expect("the GPU would reject this shader");
            assert!(!error.contains("package_shader_"), "{error}");
            assert!(!error.contains('['), "{error}");
            assert!(Arc::ptr_eq(&good, &viewer.compiled.wgsl));
        }
    }

    #[test]
    fn every_name_a_uniform_can_take_runs_on_the_gpu() {
        let uniforms = Uniforms::default();
        for word in iced::wgpu::naga::keywords::wgsl::RESERVED {
            assert!(uniforms.check(word).is_err(), "`{word}` must be refused");
        }

        let mut uniforms = Uniforms::default();
        let names = ["vec3", "color", "time2", "f32", "position", "uv", "_x"];
        for name in names {
            uniforms.add(name, Type::Float(1.0)).unwrap();
        }
        let reads = names
            .iter()
            .map(|name| format!("customs.{name}"))
            .collect::<Vec<_>>()
            .join(" + ");
        let source = format!(
            "import uniforms::customs;\n\
             fn mainBulin(pixel: bulin::Pixel) -> vec4<f32> {{\n\
                 return vec4<f32>({reads});\n\
             }}\n"
        );
        super::compile(&Program::new(source, uniforms, Some(0.0))).unwrap();
    }
}
