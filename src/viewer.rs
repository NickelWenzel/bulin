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
        Self {
            compiled: Arc::new(Compiled::from(program)),
            validating: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Shows `program` from the next frame on.
    pub fn load(&mut self, program: &Program) {
        self.compiled = Arc::new(Compiled::from(program));
        self.validating.store(true, Ordering::Relaxed);
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

impl From<&Program> for Compiled {
    fn from(program: &Program) -> Self {
        Self {
            wgsl: program.wgsl(),
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
