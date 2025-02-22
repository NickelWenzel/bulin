use crate::uniforms_editor::uniform::*;

pub type FragmentShader = String;

use std::time::Instant;
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
    Update(u32, Uniform),
    Remove(u32),
    Clear,
}

#[derive(Debug, Clone)]
pub enum TimeUpdate {
    Add(Instant),
    Remove,
    Tick,
}