use crate::uniforms_editor::uniform::*;

pub type FragmentShader = String;

use std::time::Instant;

#[derive(Debug, Clone)]
pub enum PipelineUpdate {
    Shader(FragmentShader),
    Uniforms(UniformsUpdate),
    Time(TimeUpdate),
}

#[derive(Debug, Clone)]
pub enum UniformsUpdate {
    Add(Uniform),
    Update(u32, Uniform),
    Remove(u32),
    Clear,
}

#[derive(Debug, Clone)]
pub enum TimeUpdate {
    Add,
    Remove,
    Tick(Instant),
}
