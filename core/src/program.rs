//! The program the viewer runs: the user's shader with the uniforms it reads.

use crate::uniform::{self, Type, Uniforms};

/// The shader a new project starts with.
pub const DEFAULT_SOURCE: &str = include_str!("program/default.wgsl");

/// The fullscreen-triangle vertex stage every shader is compiled with.
const PRELUDE: &str = include_str!("program/prelude.wgsl");

/// A user shader and the values of the `Customs` struct it reads.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    /// The user's fragment shader.
    pub source: String,
    pub uniforms: Uniforms,
    /// Seconds on the clock, declared as `customs.time` when present.
    pub time: Option<f32>,
}

impl Program {
    pub fn new(source: impl Into<String>, uniforms: Uniforms, time: Option<f32>) -> Self {
        Self {
            source: source.into(),
            uniforms,
            time,
        }
    }

    /// The complete WGSL module: vertex stage, `Customs` declaration and user source.
    pub fn wgsl(&self) -> String {
        let members = self
            .time
            .map(|_| "time: f32".to_owned())
            .into_iter()
            .chain(
                self.uniforms
                    .iter()
                    .map(|uniform| format!("{}: {}", uniform.name(), uniform.value.wgsl())),
            )
            .collect::<Vec<_>>();

        if members.is_empty() {
            return format!("{PRELUDE}\n\n{}", self.source);
        }

        format!(
            "{PRELUDE}\n\nstruct Customs {{\n    {},\n}}\n\n\
             @group(0) @binding(0) var<uniform> customs: Customs;\n\n{}",
            members.join(",\n    "),
            self.source
        )
    }

    /// The `Customs` struct's bytes for the GPU uniform buffer; empty without members.
    pub fn bytes(&self) -> Vec<u8> {
        uniform::pack(
            self.time
                .map(Type::Float)
                .into_iter()
                .chain(self.uniforms.iter().map(|uniform| uniform.value)),
        )
    }
}

impl Default for Program {
    fn default() -> Self {
        Self::new(DEFAULT_SOURCE, Uniforms::default(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_without_members_declares_no_customs() {
        let program = Program::default();

        assert!(!program.wgsl().contains("Customs"));
        assert!(program.wgsl().ends_with(DEFAULT_SOURCE));
        assert!(program.bytes().is_empty());
    }

    #[test]
    fn time_is_declared_first_then_the_uniforms_in_order() {
        let mut uniforms = Uniforms::default();
        uniforms.add("color", Type::Col3((1.0, 0.0, 0.0))).unwrap();
        uniforms.add("leaves", Type::Int(5)).unwrap();
        let program = Program::new("// user", uniforms, Some(2.0));

        assert!(program.wgsl().contains(
            "struct Customs {\n    time: f32,\n    color: vec3<f32>,\n    leaves: i32,\n}"
        ));
        assert!(program.wgsl().contains("var<uniform> customs: Customs;"));
        assert!(program.wgsl().ends_with("// user"));

        // `leaves` fills the four bytes `color`'s vec3 leaves free.
        let bytes = program.bytes();
        assert_eq!(bytes.len(), 32);
        assert_eq!(&bytes[0..4], &2.0f32.to_le_bytes());
        assert_eq!(&bytes[16..20], &1.0f32.to_le_bytes());
        assert_eq!(&bytes[28..32], &5i32.to_le_bytes());
    }
}
