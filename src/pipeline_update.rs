use crate::uniforms_editor::uniform::*;

pub type FragmentShader = String;

#[derive(Debug, Clone)]
pub enum PipelineUpdate {
    Shader(FragmentShader),
    Uniforms(UniformsUpdate),
}

#[derive(Debug, Clone)]
pub enum UniformsUpdate {
    Add(Uniform),
    Update(String, Uniform),
    Remove(String),
    Reset(Vec<Uniform>),
    Clear,
}
