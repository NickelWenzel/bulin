use crate::uniforms_editor::uniform::*;

pub type FragmentShader = String;

use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum PipelineUpdate {
    Shader(Arc<FragmentShader>),
    Uniforms(UniformsUpdate),
    Time(TimeUpdate),
}

#[derive(Debug, Clone)]
pub enum UniformsUpdate {
    Add(Uniform),
    Value((u32, Type)),
    Name((u32, String)),
    Remove(u32),
    Clear,
}

#[derive(Debug, Clone)]
pub enum TimeUpdate {
    Add,
    Remove,
    Value,
}