//! The program the viewer runs: the user's shader compiled with the packages it imports.

use std::borrow::Cow;

use wesl::error::{ResolveError, UsageError};
use wesl::syntax::{ModulePath, PathOrigin};
use wesl::{CompileOptions, Compiler, Resolver};

use crate::uniform::{self, Type, Uniforms};

/// The shader a new project starts with.
pub const DEFAULT_SOURCE: &str = include_str!("program/default.wesl");

/// The name of the package generated from the uniforms.
pub const UNIFORMS_PACKAGE_NAME: &str = "uniforms";

/// The name of the fixed package every shader can import.
pub const BULIN_PACKAGE_NAME: &str = "bulin";

/// The `bulin` package's source.
pub const BULIN_PACKAGE_SOURCE: &str = include_str!("program/bulin.wesl");

/// bulin's main module: the vertex stage and the fragment entry point calling `mainBulin`.
const MAIN: &str = include_str!("program/main.wesl");

/// What bulin's main module declares.
const MAIN_DECLARATIONS: [&str; 3] = ["VsOut", "vs_main", "fs_main"];

/// The function every shader defines.
const MAIN_BULIN: &str = "mainBulin";

/// A user shader and the values of the uniforms it reads.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    /// The user's shader, a WESL module.
    pub source: String,
    pub uniforms: Uniforms,
    /// Seconds on the clock, the `time` member of `customs` when present.
    pub time: Option<f32>,
}

/// Why a shader doesn't compile.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("the shader must define `fn mainBulin(pixel: bulin::Pixel) -> vec4<f32>`")]
    MissingMain,
    #[error("{0}")]
    Compile(String),
}

impl Program {
    pub fn new(source: impl Into<String>, uniforms: Uniforms, time: Option<f32>) -> Self {
        Self {
            source: source.into(),
            uniforms,
            time,
        }
    }

    /// The `uniforms` package: the `Customs` struct and its `customs` binding, or nothing when
    /// there are no members.
    pub fn uniforms_package(&self) -> String {
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
            return String::new();
        }

        format!(
            "struct Customs {{\n    {},\n}}\n\n\
             @group(0) @binding(0) var<uniform> customs: Customs;\n",
            members.join(",\n    "),
        )
    }

    /// The complete WGSL module: bulin's entry points, the shader and what it imports.
    pub fn compile(&self) -> Result<String, Error> {
        let options = CompileOptions {
            // wgsl-analyzer can't parse `public`, so packages are written without it.
            visibility: false,
            ..CompileOptions::default()
        };
        let modules = Modules {
            shader: &self.source,
            uniforms: self.uniforms_package(),
        };

        Compiler::new_with_resolver(options, modules)
            .compile_root()
            .map(|compiled| compiled.to_string())
            .map_err(|error| {
                let diagnostic = error.diagnostic();
                match &*diagnostic.error {
                    wesl::Error::UsageError(UsageError::NotFound(module, name))
                        if is_shader(module) && name == MAIN_BULIN =>
                    {
                        Error::MissingMain
                    }
                    _ => match (&diagnostic.detail, diagnostic.detail.span) {
                        (detail, Some(span)) if in_shader(detail) => {
                            let before = &self.source[..span.start.min(self.source.len())];
                            let line = before.matches('\n').count() + 1;
                            let column = before
                                .rsplit('\n')
                                .next()
                                .map_or(0, |line| line.chars().count())
                                + 1;
                            Error::Compile(format!("{line}:{column}: {}", diagnostic.error))
                        }
                        _ => Error::Compile(diagnostic.render_plain()),
                    },
                }
            })
    }

    /// The `Customs` struct's bytes for the GPU uniform buffer; empty without members.
    pub fn bytes(&self) -> Vec<u8> {
        bytes(self.time, &self.uniforms)
    }
}

impl Default for Program {
    fn default() -> Self {
        Self::new(DEFAULT_SOURCE, Uniforms::default(), None)
    }
}

/// The `Customs` struct's bytes for `time` and `uniforms`, without building a [`Program`].
pub fn bytes(time: Option<f32>, uniforms: &Uniforms) -> Vec<u8> {
    uniform::pack(
        time.map(Type::Float)
            .into_iter()
            .chain(uniforms.iter().map(|uniform| uniform.value)),
    )
}

/// The modules a program compiles from, all in memory.
struct Modules<'a> {
    shader: &'a str,
    uniforms: String,
}

impl Resolver for Modules<'_> {
    fn resolve_source<'a>(&'a self, path: &ModulePath) -> Result<Cow<'a, str>, ResolveError> {
        let source = match (&path.origin, path.components.as_slice()) {
            (PathOrigin::Absolute, []) => MAIN,
            _ if is_shader(path) => self.shader,
            (PathOrigin::Package(package), []) if package == UNIFORMS_PACKAGE_NAME => {
                &self.uniforms
            }
            (PathOrigin::Package(package), []) if package == BULIN_PACKAGE_NAME => {
                BULIN_PACKAGE_SOURCE
            }
            _ => {
                return Err(ResolveError::ModuleNotFound(
                    path.clone(),
                    format!(
                        "bulin provides only the `{UNIFORMS_PACKAGE_NAME}` and \
                         `{BULIN_PACKAGE_NAME}` packages"
                    ),
                ));
            }
        };
        Ok(Cow::Borrowed(source))
    }

    fn display_name(&self, path: &ModulePath) -> Option<String> {
        Some(if is_shader(path) {
            "shader".to_owned()
        } else {
            path.to_string()
        })
    }
}

/// Whether a diagnostic's span points into the shader's text. Validation errors name only the
/// declaration they're in, and condcomp errors carry a span without either, so a span counts as
/// the shader's when the shader module is named, or when the declaration isn't one of the main
/// module's.
fn in_shader(detail: &wesl::error::Detail) -> bool {
    match (&detail.module_path, &detail.declaration) {
        (Some(module), _) => is_shader(module),
        (None, Some(declaration)) => !MAIN_DECLARATIONS.contains(&declaration.as_str()),
        (None, None) => false,
    }
}

/// Whether `path` is the user's shader, `package::shader`.
fn is_shader(path: &ModulePath) -> bool {
    matches!(
        (&path.origin, path.components.as_slice()),
        (PathOrigin::Absolute, [module]) if module == "shader"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_shader_compiles_with_both_entry_points() {
        let wgsl = Program::default().compile().unwrap();

        assert!(wgsl.contains("fn vs_main("));
        assert!(wgsl.contains("fn fs_main("));
        assert!(!wgsl.contains("customs"));
    }

    #[test]
    fn a_shader_reads_imported_uniforms() {
        let mut uniforms = Uniforms::default();
        uniforms.add("color", Type::Col3((1.0, 0.0, 0.0))).unwrap();
        let source = "import uniforms::customs;\n\
                      fn mainBulin(pixel: bulin::Pixel) -> vec4<f32> {\n\
                          return vec4<f32>(customs.color * sin(customs.time), pixel.uv.x);\n\
                      }\n";
        let wgsl = Program::new(source, uniforms, Some(1.0)).compile().unwrap();

        assert!(wgsl.contains("@group(0) @binding(0)"));
        assert!(wgsl.contains("time: f32"));
        assert!(wgsl.contains("color: vec3<f32>"));
    }

    #[test]
    fn time_is_declared_first_then_the_uniforms_in_order() {
        let mut uniforms = Uniforms::default();
        uniforms.add("color", Type::Col3((1.0, 0.0, 0.0))).unwrap();
        uniforms.add("leaves", Type::Int(5)).unwrap();
        let program = Program::new("", uniforms, Some(2.0));

        assert!(program.uniforms_package().starts_with(
            "struct Customs {\n    time: f32,\n    color: vec3<f32>,\n    leaves: i32,\n}"
        ));

        // `leaves` fills the four bytes `color`'s vec3 leaves free.
        let bytes = program.bytes();
        assert_eq!(bytes.len(), 32);
        assert_eq!(&bytes[0..4], &2.0f32.to_le_bytes());
        assert_eq!(&bytes[16..20], &1.0f32.to_le_bytes());
        assert_eq!(&bytes[28..32], &5i32.to_le_bytes());
    }

    #[test]
    fn a_program_without_members_has_an_empty_uniforms_package() {
        assert!(Program::default().uniforms_package().is_empty());
        assert!(Program::default().bytes().is_empty());
    }

    #[test]
    fn a_shader_without_main_bulin_is_told_what_to_define() {
        let fragment = "@fragment\n\
                        fn fs_main() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }\n";

        assert_eq!(
            Program::new(fragment, Uniforms::default(), None).compile(),
            Err(Error::MissingMain)
        );
    }

    #[test]
    fn a_syntax_error_reports_its_line_and_column_in_the_shader() {
        let broken = "// ünïcode\nfn mainBulin(pixel: bulin::Pixel) -> vec4<f32> { return }\n";

        let Err(Error::Compile(message)) =
            Program::new(broken, Uniforms::default(), None).compile()
        else {
            panic!("a syntax error must not compile");
        };
        assert!(message.starts_with("2:"), "{message}");
        assert!(!message.contains('\n'), "{message}");
    }

    #[test]
    fn an_undefined_name_reports_its_line_in_the_shader() {
        let source = "fn mainBulin(pixel: bulin::Pixel) -> vec4<f32> {\n    return nothing;\n}\n";

        let Err(Error::Compile(message)) =
            Program::new(source, Uniforms::default(), None).compile()
        else {
            panic!("an undefined name must not compile");
        };
        assert!(message.starts_with("2:"), "{message}");
    }
}
