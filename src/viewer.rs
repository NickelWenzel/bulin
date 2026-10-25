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
    // Set on every change and cleared by the pipeline once no edited shader
    // awaits validation, keeping frames coming until its result is applied
    validating: Arc<AtomicBool>,
}

impl State {
    pub fn new(program: &Program) -> Self {
        let mut state = Self {
            compiled: Arc::new(Compiled::default()),
            validating: Arc::new(AtomicBool::new(true)),
        };
        state.load(program);
        state
    }

    /// Shows `program` from the next frame on. A program that doesn't compile leaves the last
    /// one showing.
    pub fn load(&mut self, program: &Program) {
        match program.compile() {
            Ok(wgsl) => {
                self.compiled = Arc::new(Compiled {
                    wgsl,
                    uniforms: program.bytes(),
                });
                self.validating.store(true, Ordering::Relaxed);
            }
            Err(error) => tracing::warn!("the shader doesn't compile: {error}"),
        }
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
    wgsl: String,
    uniforms: Vec<u8>,
}

impl Default for Compiled {
    /// The default program, which always compiles.
    fn default() -> Self {
        let program = Program::default();
        Self {
            wgsl: program
                .compile()
                .expect("invariant: the default program compiles"),
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
    use iced::wgpu::naga;

    use crate::core::{Program, Project};

    /// What the GPU checks before the pipeline accepts a shader.
    fn validate(wgsl: &str) {
        let module = naga::front::wgsl::parse_str(wgsl).unwrap_or_else(|error| {
            panic!("{}\n{wgsl}", error.emit_to_string(wgsl));
        });
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|error| panic!("{error:?}\n{wgsl}"));
    }

    #[test]
    fn compiled_programs_are_valid_wgsl() {
        validate(&Program::default().compile().unwrap());

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
            validate(&program.compile().unwrap());
        }
    }
}
