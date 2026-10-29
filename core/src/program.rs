//! The program the viewer runs: a main file compiled with the modules it imports and the packages
//! bulin provides.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use wesl::error::{ResolveError, UsageError};
use wesl::mangler::EscapeMangler;
use wesl::syntax::{ModulePath, PathOrigin, TranslationUnit};
use wesl::{CompileOptions, Compiler, Mangler, Resolver};

use crate::folder::Path;
use crate::uniform::{self, KEYWORDS, Type, Uniforms};

/// The shader a new project starts with.
pub const DEFAULT_SOURCE: &str = include_str!("program/default.wesl");

/// The name of the package generated from the uniforms.
pub const UNIFORMS_PACKAGE_NAME: &str = "uniforms";

/// The name of the fixed package every shader can import.
pub const BULIN_PACKAGE_NAME: &str = "bulin";

/// The `bulin` package's source.
pub const BULIN_PACKAGE_SOURCE: &str = include_str!("program/bulin.wesl");

/// Where bulin's generated main module sits in the package, next to the folder's modules. Absolute
/// imports in it reach the folder's modules, which they couldn't from another package.
const MAIN_MODULE: &str = "__bulin_main";

/// What bulin's main module declares after its imports: the vertex stage and the fragment entry
/// point calling `mainBulin`. The names carry no underscores, so they can't be taken for mangled
/// names, and differ from the usual `VsOut`/`fs_main` a user file may declare.
const MAIN_BODY: &str = "struct BulinVarying {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn bulinVertex(@builtin(vertex_index) i: u32) -> BulinVarying {
    // Fullscreen triangle
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: BulinVarying;
    out.uv = uv;
    out.pos = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    return out;
}

@fragment
fn bulinFragment(in: BulinVarying) -> @location(0) vec4<f32> {
    return mainBulin(Pixel(in.uv, in.pos));
}
";

/// What bulin's main module declares.
const MAIN_DECLARATIONS: [&str; 3] = ["BulinVarying", "bulinVertex", "bulinFragment"];

/// The function every main file defines.
const MAIN_BULIN: &str = "mainBulin";

/// The file a project's shader is compiled as.
const PROJECT_FILE: &str = "shader.wesl";

/// A main file with the files it may import, and the values of the uniforms they read.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    /// Every file of the folder, shared with whoever keeps them so a program is cheap to build.
    sources: Arc<BTreeMap<Path, String>>,
    /// The package root, `None` for the folder itself. Only files under it are modules.
    root: Option<Path>,
    main: Module,
    uniforms: Uniforms,
    /// Seconds on the clock, the `time` member of `customs` when present.
    time: Option<f32>,
    /// Whether this is a project's one shader, whose errors name no file.
    project: bool,
}

/// A compiled program.
#[derive(Debug, Clone, PartialEq)]
pub struct Compiled {
    /// The complete WGSL module: bulin's entry points, the main file and what it imports.
    pub wgsl: String,
    /// The folder's modules the compile read, so a change to any other file can't affect it.
    pub read: BTreeSet<Module>,
}

/// A folder file seen as a WESL module of the package, such as `noise::fbm` for `noise/fbm.wesl`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Module(Vec<String>);

/// Why a file can't be a module.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum ModuleError {
    #[error("`{0}` isn't a `.wesl` or `.wgsl` file")]
    NotShader(Path),
    #[error("`{file}` isn't inside the package root `{root}`")]
    OutsideRoot { file: Path, root: Path },
    #[error("`{0}` isn't a valid module name")]
    Invalid(String),
    #[error("`{0}` is a WGSL keyword")]
    Keyword(String),
    #[error("`{0}` is reserved in WGSL")]
    Reserved(String),
}

/// Why a shader doesn't compile.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("the shader must define `fn mainBulin(pixel: bulin::Pixel) -> vec4<f32>`")]
    MissingMain,
    /// An error in the file it names, at the line and column given, both counted from 1. A project
    /// names no file.
    #[error("{}", located(file.as_ref(), *line_column, message))]
    Compile {
        file: Option<Path>,
        line_column: Option<(usize, usize)>,
        message: String,
    },
    #[error(transparent)]
    Module(#[from] ModuleError),
}

impl Module {
    /// The module `file` is in the package rooted at `root`, `None` being the folder itself.
    ///
    /// `package.wesl` at the root is the package's root module. Every other file must be a
    /// `.wesl` or `.wgsl` file under the root whose directories and name are valid WGSL names.
    pub fn of(root: Option<&Path>, file: &Path) -> Result<Self, ModuleError> {
        let relative = match root {
            Some(root) => file
                .strip_prefix(root)
                .ok_or_else(|| ModuleError::OutsideRoot {
                    file: file.clone(),
                    root: root.clone(),
                })?,
            None => file.as_str(),
        };
        let Some(stem) = relative
            .strip_suffix(".wesl")
            .or_else(|| relative.strip_suffix(".wgsl"))
        else {
            return Err(ModuleError::NotShader(file.clone()));
        };
        if stem == "package" {
            return Ok(Self(Vec::new()));
        }

        // The identifier rule refuses a leading `__`, which keeps `__bulin_main` for bulin.
        stem.split('/')
            .map(|component| {
                if !uniform::is_identifier(component) {
                    Err(ModuleError::Invalid(component.to_owned()))
                } else if KEYWORDS.contains(&component) {
                    Err(ModuleError::Keyword(component.to_owned()))
                } else if wgsl_types::idents::RESERVED_WORDS.contains(&component) {
                    Err(ModuleError::Reserved(component.to_owned()))
                } else {
                    Ok(component.to_owned())
                }
            })
            .collect::<Result<_, _>>()
            .map(Self)
    }

    fn path(&self) -> ModulePath {
        ModulePath::new(PathOrigin::Absolute, self.0.clone())
    }
}

impl Program {
    /// A project's program: its one shader as the main file.
    pub fn project(source: impl Into<String>, uniforms: Uniforms, time: Option<f32>) -> Self {
        let file = Path::new(PROJECT_FILE).expect("invariant: the project file is a valid path");
        let main = Module::of(None, &file).expect("invariant: the project file is a valid module");
        Self {
            sources: Arc::new(BTreeMap::from([(file, source.into())])),
            root: None,
            main,
            uniforms,
            time,
            project: true,
        }
    }

    /// A folder's program: `main` with the files under `root` it imports, out of `sources`.
    pub fn folder(
        sources: Arc<BTreeMap<Path, String>>,
        root: Option<Path>,
        main: &Path,
        uniforms: Uniforms,
        time: Option<f32>,
    ) -> Result<Self, Error> {
        let main = Module::of(root.as_ref(), main)?;
        Ok(Self {
            sources,
            root,
            main,
            uniforms,
            time,
            project: false,
        })
    }

    /// The `uniforms` package: the `Customs` struct and its `customs` binding, or nothing when
    /// there are no members.
    pub fn uniforms_package(&self) -> String {
        uniforms_package(self.time, &self.uniforms)
    }

    /// Compiles the main file with what it imports, or says what is wrong and where.
    pub fn compile(&self) -> Result<Compiled, Error> {
        let options = CompileOptions {
            // wgsl-analyzer can't parse `public`, so packages are written without it.
            visibility: false,
            ..CompileOptions::default()
        };
        let main = self.main.path().join([MAIN_BULIN.to_owned()]);
        let sources = Sources {
            modules: self.modules(),
            main: format!("import bulin::Pixel;\nimport {main};\n\n{MAIN_BODY}"),
            uniforms: self.uniforms_package(),
            project: self.project,
            read: RefCell::default(),
        };

        let result = Compiler::new_with_resolver(options, &sources).compile_module(
            &ModulePath::new(PathOrigin::Absolute, vec![MAIN_MODULE.to_owned()]),
        );
        let read = sources.read.take();
        let modules = &sources.modules;
        let file = |path: &ModulePath| {
            modules
                .get(path)
                .filter(|_| !self.project)
                .map(|(_, file, _)| file.clone())
        };

        let error = match result {
            Ok(compiled) => {
                return Ok(Compiled {
                    wgsl: compiled.to_string(),
                    read: read
                        .iter()
                        .filter_map(|path| modules.get(path))
                        .map(|(module, _, _)| module.clone())
                        .collect(),
                });
            }
            Err(error) => error.diagnostic(),
        };
        let message = error.error.to_string();
        let detail = &error.detail;
        let missing = match &*error.error {
            wesl::Error::ResolveError(ResolveError::ModuleNotFound(module, _)) => {
                Some((module, None))
            }
            wesl::Error::UsageError(UsageError::NotFound(module, item)) => {
                Some((module, Some(item.as_str())))
            }
            _ => None,
        };

        Err(match (missing, &*error.error) {
            (Some((module, Some(MAIN_BULIN))), _) if *module == self.main.path() => {
                Error::MissingMain
            }
            // wgsl-parse keeps no span for imports, so an import typo names only the file.
            (Some((module, item)), _) => {
                let importer = read.iter().find_map(|path| {
                    let (_, _, source) = modules.get(path)?;
                    let unit = source.parse::<TranslationUnit>().ok()?;
                    wesl::pass::flatten_imports(&unit.imports, path)
                        .values()
                        .any(|import| {
                            import.path == *module
                                && item.is_none_or(|item| *import.ident.name() == item)
                        })
                        .then_some(path)
                });
                Error::Compile {
                    file: match importer {
                        Some(importer) => file(importer),
                        None => item.and_then(|_| file(module)),
                    },
                    line_column: None,
                    message,
                }
            }
            (None, wesl::Error::UsageError(UsageError::Visibility { decl, .. })) => {
                Error::Compile {
                    file: file(&decl.0),
                    line_column: None,
                    message,
                }
            }
            (None, wesl::Error::ResolveError(_)) => Error::Compile {
                file: None,
                line_column: None,
                message,
            },
            _ => match (&detail.module_path, &detail.declaration, detail.span) {
                (Some(path), _, span) if let Some((_, _, source)) = modules.get(path) => {
                    Error::Compile {
                        file: file(path),
                        line_column: span.map(|span| line_column(source, span.start)),
                        message,
                    }
                }
                (Some(path), _, _)
                    if path.origin == PathOrigin::Absolute && path.components == [MAIN_MODULE] =>
                {
                    Error::MissingMain
                }
                // The sourcemap names the module of every mangled declaration, so a declaration
                // without one is in the main module, whose names stay unmangled.
                (None, Some(declaration), _)
                    if MAIN_DECLARATIONS.contains(&declaration.as_str()) =>
                {
                    Error::MissingMain
                }
                // Errors while loading a module, such as condcomp and validation errors, carry
                // only a span, and the module they're in is the last one read.
                (None, _, Some(span))
                    if let Some(path) = read.last()
                        && let Some((_, _, source)) = modules.get(path) =>
                {
                    Error::Compile {
                        file: file(path),
                        line_column: Some(line_column(source, span.start)),
                        message,
                    }
                }
                _ => Error::Compile {
                    file: None,
                    line_column: None,
                    message: error.render_plain(),
                },
            },
        })
    }

    /// `message` with wesl's mangled names turned back into the names the files use, and the
    /// file of the first name that belongs to one. Only names of the folder's modules and of the
    /// `uniforms` and `bulin` packages are turned back, so ordinary `snake_case` stays as it is.
    /// The main file's names become bare, as it declares them.
    pub fn unmangle(&self, message: &str) -> (String, Option<Path>) {
        let modules = self.modules();
        let mut file = None;
        let mut unmangled = String::with_capacity(message.len());
        let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_';

        let mut rest = message;
        while let Some(start) = rest.find(is_word) {
            unmangled.push_str(&rest[..start]);
            rest = &rest[start..];
            let end = rest.find(|c| !is_word(c)).unwrap_or(rest.len());
            let (word, after) = rest.split_at(end);
            rest = after;

            let name = EscapeMangler
                .unmangle(word)
                .and_then(|(path, item)| match &path.origin {
                    PathOrigin::Absolute => {
                        let (module, path_file, _) = modules.get(&path)?;
                        if !self.project && file.is_none() {
                            file = Some(path_file.clone());
                        }
                        Some(if *module == self.main {
                            item
                        } else {
                            format!("{path}::{item}")
                        })
                    }
                    PathOrigin::Package(package)
                        if path.components.is_empty()
                            && [UNIFORMS_PACKAGE_NAME, BULIN_PACKAGE_NAME]
                                .contains(&package.as_str()) =>
                    {
                        Some(format!("{package}::{item}"))
                    }
                    _ => None,
                });
            unmangled.push_str(name.as_deref().unwrap_or(word));
        }
        unmangled.push_str(rest);

        (unmangled, file)
    }

    /// The `Customs` struct's bytes for the GPU uniform buffer; empty without members.
    pub fn bytes(&self) -> Vec<u8> {
        bytes(self.time, &self.uniforms)
    }

    /// The modules of the files under the root, each read from its `.wesl` file when there also
    /// is a `.wgsl` one.
    fn modules(&self) -> Modules<'_> {
        let mut modules = BTreeMap::new();
        for (path, source) in self.sources.iter() {
            let Ok(module) = Module::of(self.root.as_ref(), path) else {
                continue;
            };
            match modules.entry(module) {
                Entry::Vacant(entry) => {
                    entry.insert((path, source.as_str()));
                }
                Entry::Occupied(mut entry) => {
                    if path.extension() == Some("wesl") {
                        entry.insert((path, source.as_str()));
                    }
                }
            }
        }
        Modules(modules)
    }
}

impl Default for Program {
    fn default() -> Self {
        Self::project(DEFAULT_SOURCE, Uniforms::default(), None)
    }
}

/// The `uniforms` package for `time` and `uniforms`, without building a [`Program`]: the
/// `Customs` struct and its `customs` binding, or nothing when there are no members.
pub fn uniforms_package(time: Option<f32>, uniforms: &Uniforms) -> String {
    let members = time
        .map(|_| "time: f32".to_owned())
        .into_iter()
        .chain(
            uniforms
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

/// The `Customs` struct's bytes for `time` and `uniforms`, without building a [`Program`].
pub fn bytes(time: Option<f32>, uniforms: &Uniforms) -> Vec<u8> {
    uniform::pack(
        time.map(Type::Float)
            .into_iter()
            .chain(uniforms.iter().map(|uniform| uniform.value)),
    )
}

/// The folder's modules a compile can read, with each one's file and text.
struct Modules<'a>(BTreeMap<Module, (&'a Path, &'a str)>);

impl<'a> Modules<'a> {
    /// The folder module `path` names, if it is one.
    fn get(&self, path: &ModulePath) -> Option<(&Module, &'a Path, &'a str)> {
        if path.origin != PathOrigin::Absolute {
            return None;
        }
        self.0
            .iter()
            .find(|(module, _)| module.0 == path.components)
            .map(|(module, (file, source))| (module, *file, *source))
    }
}

/// Everything a compile reads, all in memory, noting each module it serves.
struct Sources<'a> {
    modules: Modules<'a>,
    /// The generated main module.
    main: String,
    uniforms: String,
    project: bool,
    /// The modules served so far, in order.
    read: RefCell<Vec<ModulePath>>,
}

impl Resolver for Sources<'_> {
    fn resolve_source<'a>(&'a self, path: &ModulePath) -> Result<Cow<'a, str>, ResolveError> {
        let source = match (&path.origin, path.components.as_slice()) {
            (PathOrigin::Absolute, [module]) if module == MAIN_MODULE => &self.main,
            (PathOrigin::Absolute, _) if let Some((_, _, source)) = self.modules.get(path) => {
                source
            }
            (PathOrigin::Package(package), []) if package == UNIFORMS_PACKAGE_NAME => {
                &self.uniforms
            }
            (PathOrigin::Package(package), []) if package == BULIN_PACKAGE_NAME => {
                BULIN_PACKAGE_SOURCE
            }
            (PathOrigin::Absolute, _) if !self.project => {
                return Err(ResolveError::ModuleNotFound(
                    path.clone(),
                    "no `.wesl` or `.wgsl` file in the package defines it".to_owned(),
                ));
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
        self.read.borrow_mut().push(path.clone());
        Ok(Cow::Borrowed(source))
    }

    fn display_name(&self, path: &ModulePath) -> Option<String> {
        Some(match self.modules.get(path) {
            Some(_) if self.project => "shader".to_owned(),
            Some((_, file, _)) => file.to_string(),
            None => path.to_string(),
        })
    }
}

/// How a compile error reads: its file and position first, when known.
fn located(file: Option<&Path>, line_column: Option<(usize, usize)>, message: &str) -> String {
    match (file, line_column) {
        (Some(file), Some((line, column))) => format!("{file}:{line}:{column}: {message}"),
        (None, Some((line, column))) => format!("{line}:{column}: {message}"),
        (Some(file), None) => format!("{file}: {message}"),
        (None, None) => message.to_owned(),
    }
}

/// The line and column of byte `offset` in `source`, both counted from 1, in characters.
fn line_column(source: &str, offset: usize) -> (usize, usize) {
    let before = source.get(..offset).unwrap_or(source);
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .map_or(0, |line| line.chars().count())
        + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN_READS_FBM: &str = "import package::noise::fbm;\n\
        fn mainBulin(pixel: bulin::Pixel) -> vec4<f32> {\n\
            return vec4<f32>(fbm(pixel.uv.x));\n\
        }\n";

    fn path(text: &str) -> Path {
        Path::new(text).unwrap()
    }

    fn folder(files: &[(&str, &str)], main: &str) -> Program {
        let sources = files
            .iter()
            .map(|(file, source)| (path(file), source.to_string()))
            .collect();
        Program::folder(
            Arc::new(sources),
            None,
            &path(main),
            Uniforms::default(),
            None,
        )
        .unwrap()
    }

    fn module(file: &str) -> Module {
        Module::of(None, &path(file)).unwrap()
    }

    #[test]
    fn project_programs_compile_with_bulin_entry_points() {
        let wgsl = Program::default().compile().unwrap().wgsl;

        assert!(wgsl.contains("fn bulinVertex("));
        assert!(wgsl.contains("fn bulinFragment("));
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
        let wgsl = Program::project(source, uniforms, Some(1.0))
            .compile()
            .unwrap()
            .wgsl;

        assert!(wgsl.contains("@group(0) @binding(0)"));
        assert!(wgsl.contains("time: f32"));
        assert!(wgsl.contains("color: vec3<f32>"));
    }

    #[test]
    fn time_is_declared_first_then_the_uniforms_in_order() {
        let mut uniforms = Uniforms::default();
        uniforms.add("color", Type::Col3((1.0, 0.0, 0.0))).unwrap();
        uniforms.add("leaves", Type::Int(5)).unwrap();
        let program = Program::project("", uniforms, Some(2.0));

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
    fn a_missing_main_bulin_is_missing_main() {
        let fragment = "@fragment\n\
                        fn fs_main() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }\n";

        assert_eq!(
            Program::project(fragment, Uniforms::default(), None).compile(),
            Err(Error::MissingMain)
        );
    }

    #[test]
    fn project_mode_reports_no_file() {
        let broken = "// ünïcode\nfn mainBulin(pixel: bulin::Pixel) -> vec4<f32> { return }\n";
        let undefined =
            "fn mainBulin(pixel: bulin::Pixel) -> vec4<f32> {\n    return nothing;\n}\n";

        for source in [broken, undefined] {
            let Err(error @ Error::Compile { file: None, .. }) =
                Program::project(source, Uniforms::default(), None).compile()
            else {
                panic!("`{source}` must not compile, and name no file");
            };
            let message = error.to_string();
            assert!(message.starts_with("2:"), "{message}");
            assert!(!message.contains('\n'), "{message}");
        }
    }

    #[test]
    fn nested_imports_resolve() {
        let program = folder(
            &[
                ("main.wesl", MAIN_READS_FBM),
                (
                    "noise.wesl",
                    "import package::util::hash;\nfn fbm(x: f32) -> f32 { return hash(x); }\n",
                ),
                ("util.wesl", "fn hash(x: f32) -> f32 { return fract(x); }\n"),
                ("unused.wesl", "fn unused() {}\n"),
            ],
            "main.wesl",
        );

        let compiled = program.compile().unwrap();

        assert_eq!(
            compiled.read,
            BTreeSet::from([
                module("main.wesl"),
                module("noise.wesl"),
                module("util.wesl")
            ])
        );
    }

    #[test]
    fn wesl_wins_over_wgsl_and_falls_back() {
        let main = "import package::a::value;\n\
                    fn mainBulin(pixel: bulin::Pixel) -> vec4<f32> {\n\
                        return vec4<f32>(value());\n\
                    }\n";
        let mut files = vec![
            ("main.wesl", main),
            ("a.wgsl", "fn value() -> f32 { return 2.0; }\n"),
            ("a.wesl", "fn value() -> f32 { return 1.0; }\n"),
        ];

        let wgsl = folder(&files, "main.wesl").compile().unwrap().wgsl;
        assert!(wgsl.contains("return 1.0"), "{wgsl}");

        files.retain(|(file, _)| *file != "a.wesl");
        let wgsl = folder(&files, "main.wesl").compile().unwrap().wgsl;
        assert!(wgsl.contains("return 2.0"), "{wgsl}");
    }

    #[test]
    fn module_of_rules() {
        let of = |file: &str| Module::of(None, &path(file));

        assert_eq!(
            of("my-shader.wesl"),
            Err(ModuleError::Invalid("my-shader".into()))
        );
        assert_eq!(of("fn.wesl"), Err(ModuleError::Keyword("fn".into())));
        assert_eq!(of("a/self.wesl"), Err(ModuleError::Reserved("self".into())));
        assert_eq!(
            of("__bulin_main.wesl"),
            Err(ModuleError::Invalid("__bulin_main".into()))
        );
        assert_eq!(of("x.txt"), Err(ModuleError::NotShader(path("x.txt"))));
        assert_eq!(
            Module::of(Some(&path("shaders")), &path("other/a.wesl")),
            Err(ModuleError::OutsideRoot {
                file: path("other/a.wesl"),
                root: path("shaders"),
            })
        );

        assert_eq!(of("a/b.wesl"), Ok(Module(vec!["a".into(), "b".into()])));
        assert_eq!(
            Module::of(Some(&path("shaders")), &path("shaders/b.wgsl")),
            Ok(Module(vec!["b".into()]))
        );
        assert_eq!(of("package.wesl"), Ok(Module(Vec::new())));
    }

    #[test]
    fn a_root_package_wesl_can_be_the_main_file() {
        let main = "fn mainBulin(pixel: bulin::Pixel) -> vec4<f32> { return vec4<f32>(1.0); }\n";

        let compiled = folder(&[("package.wesl", main)], "package.wesl")
            .compile()
            .unwrap();

        assert_eq!(compiled.read, BTreeSet::from([module("package.wesl")]));
    }

    #[test]
    fn a_folder_main_file_must_be_a_module() {
        let sources = Arc::new(BTreeMap::new());

        assert_eq!(
            Program::folder(
                sources,
                None,
                &path("my-shader.wesl"),
                Uniforms::default(),
                None
            ),
            Err(Error::Module(ModuleError::Invalid("my-shader".into())))
        );
    }

    #[test]
    fn a_usage_error_names_its_file() {
        let main = "import package::lib::nope;\n\
                    fn mainBulin(pixel: bulin::Pixel) -> vec4<f32> {\n\
                        return vec4<f32>(nope());\n\
                    }\n";
        let program = folder(
            &[("main.wesl", main), ("lib.wesl", "fn other() {}\n")],
            "main.wesl",
        );

        let Err(Error::Compile {
            file, line_column, ..
        }) = program.compile()
        else {
            panic!("a missing declaration must not compile");
        };
        assert_eq!(file, Some(path("main.wesl")));
        assert_eq!(line_column, None);
    }

    #[test]
    fn a_parse_error_in_an_imported_file_names_it_with_line_and_column() {
        let program = folder(
            &[
                ("main.wesl", MAIN_READS_FBM),
                ("noise.wesl", "// noise\nfn fbm(x: f32) -> f32 { return }\n"),
            ],
            "main.wesl",
        );

        let error = program.compile().unwrap_err();
        let Error::Compile {
            file,
            line_column: Some((2, _)),
            ..
        } = &error
        else {
            panic!("{error:?}");
        };
        assert_eq!(file, &Some(path("noise.wesl")));
        assert!(error.to_string().starts_with("noise.wesl:2:"), "{error}");
    }

    #[test]
    fn a_link_time_error_names_its_file() {
        let program = folder(
            &[
                ("main.wesl", MAIN_READS_FBM),
                (
                    "noise.wesl",
                    "fn twice(x: f32) -> f32 { return x * 2.0; }\n\
                     fn fbm(x: f32) -> f32 {\n    return twice(x, 2.0);\n}\n",
                ),
            ],
            "main.wesl",
        );

        // An undefined name is caught while loading; a call with the wrong number of arguments
        // only once the modules are linked.
        let undefined = folder(
            &[
                ("main.wesl", MAIN_READS_FBM),
                (
                    "noise.wesl",
                    "fn fbm(x: f32) -> f32 {\n    return nothing;\n}\n",
                ),
            ],
            "main.wesl",
        );
        for program in [program, undefined] {
            let error = program.compile().unwrap_err();
            let Error::Compile { file, .. } = &error else {
                panic!("{error:?}");
            };
            assert_eq!(file, &Some(path("noise.wesl")), "{error}");
        }
    }

    #[test]
    fn a_bad_main_bulin_signature_is_missing_main() {
        let main = "fn mainBulin() -> vec4<f32> { return vec4<f32>(1.0); }\n";

        assert_eq!(
            folder(&[("main.wesl", main)], "main.wesl").compile(),
            Err(Error::MissingMain)
        );
    }

    #[test]
    fn a_load_time_validation_error_names_the_last_module_read() {
        for noise in [
            "fn fbm(x: f32) -> f32 { return x; }\n@if(1.0) fn other() {}\n",
            "fn fbm(x: f32) -> f32 { return x; }\nfn other() -> f32 { return nothing; }\n",
        ] {
            let program = folder(
                &[("main.wesl", MAIN_READS_FBM), ("noise.wesl", noise)],
                "main.wesl",
            );

            let error = program.compile().unwrap_err();
            let Error::Compile {
                file,
                line_column: Some((2, _)),
                ..
            } = &error
            else {
                panic!("{error:?}");
            };
            assert_eq!(file, &Some(path("noise.wesl")));
        }
    }

    #[test]
    fn user_vs_out_and_fs_main_are_not_main() {
        let main = "struct VsOut { x: f32 }\n\
                    fn fs_main() -> f32 { return 1.0; }\n\
                    fn mainBulin(pixel: bulin::Pixel) -> vec4<f32> {\n\
                        return vec4<f32>(fs_main());\n\
                    }\n";
        folder(&[("main.wesl", main)], "main.wesl")
            .compile()
            .unwrap();

        let broken = main.replace("return 1.0", "return nothing");
        let error = folder(&[("main.wesl", &broken)], "main.wesl")
            .compile()
            .unwrap_err();
        let Error::Compile { file, .. } = &error else {
            panic!("{error:?}");
        };
        assert_eq!(file, &Some(path("main.wesl")));
    }

    #[test]
    fn import_typos_blame_the_importer() {
        let main = |import: &str| {
            format!(
                "import {import};\n\
                 fn mainBulin(pixel: bulin::Pixel) -> vec4<f32> {{\n\
                     return vec4<f32>(f());\n\
                 }}\n"
            )
        };

        for import in ["package::nosuch::f", "package::lib::f"] {
            let main = main(import);
            let program = folder(
                &[("main.wesl", &main), ("lib.wesl", "fn other() {}\n")],
                "main.wesl",
            );

            let error = program.compile().unwrap_err();
            let Error::Compile {
                file, line_column, ..
            } = &error
            else {
                panic!("{error:?}");
            };
            assert_eq!(file, &Some(path("main.wesl")), "{import}: {error}");
            assert_eq!(line_column, &None);
        }
    }

    #[test]
    fn unmangle_maps_known_modules_only() {
        let program = folder(
            &[("main.wesl", MAIN_READS_FBM), ("noise.wesl", "")],
            "main.wesl",
        );

        assert_eq!(
            program.unmangle("base_color"),
            ("base_color".to_owned(), None)
        );
        assert_eq!(
            program.unmangle("`package_noise_fbm` is invalid"),
            (
                "`package::noise::fbm` is invalid".to_owned(),
                Some(path("noise.wesl"))
            )
        );
        assert_eq!(
            program.unmangle("package_main_mainBulin"),
            ("mainBulin".to_owned(), Some(path("main.wesl")))
        );
        assert_eq!(
            program.unmangle("uniforms_customs"),
            ("uniforms::customs".to_owned(), None)
        );
        assert_eq!(
            Program::default().unmangle("package_shader_mainBulin"),
            ("mainBulin".to_owned(), None)
        );
    }

    #[test]
    fn read_is_absent_on_failure() {
        let program = folder(
            &[("main.wesl", MAIN_READS_FBM), ("noise.wesl", "fn fbm(")],
            "main.wesl",
        );

        assert!(program.compile().is_err());
    }

    #[test]
    fn the_root_filters_sources_without_a_rescan() {
        let main = "import package::x::f;\n\
                    fn mainBulin(pixel: bulin::Pixel) -> vec4<f32> {\n\
                        return vec4<f32>(f());\n\
                    }\n";
        let sources = Arc::new(BTreeMap::from([
            (path("shaders/main.wesl"), main.to_owned()),
            (
                path("other/x.wesl"),
                "fn f() -> f32 { return 1.0; }\n".to_owned(),
            ),
        ]));
        let program = |root: Option<Path>, main: &str| {
            Program::folder(
                Arc::clone(&sources),
                root,
                &path(main),
                Uniforms::default(),
                None,
            )
            .unwrap()
        };

        let error = program(Some(path("shaders")), "shaders/main.wesl")
            .compile()
            .unwrap_err();
        let Error::Compile { file, .. } = &error else {
            panic!("{error:?}");
        };
        assert_eq!(file, &Some(path("shaders/main.wesl")));

        let sources_with_x = Arc::new(BTreeMap::from([
            (path("shaders/main.wesl"), main.to_owned()),
            (
                path("shaders/x.wesl"),
                "fn f() -> f32 { return 1.0; }\n".to_owned(),
            ),
            (path("other/x.wesl"), "fn broken(".to_owned()),
        ]));
        Program::folder(
            sources_with_x,
            Some(path("shaders")),
            &path("shaders/main.wesl"),
            Uniforms::default(),
            None,
        )
        .unwrap()
        .compile()
        .unwrap();
    }
}
