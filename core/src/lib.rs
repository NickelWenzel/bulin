//! What bulin edits, runs and saves, free of any GUI or GPU: the uniforms a shader reads, the
//! program the viewer compiles from them, the clock behind the built-in `time` uniform, the
//! project file, and the settings the editor keeps across projects.

pub mod clock;
pub mod color;
pub mod folder;
pub mod program;
pub mod project;
pub mod rotation;
pub mod settings;
pub mod uniform;

pub use clock::Clock;
pub use program::Program;
pub use project::Project;
pub use settings::Settings;
pub use uniform::Uniforms;
