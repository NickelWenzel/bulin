//! What bulin edits, runs and saves, free of any GUI or GPU: the uniforms a shader reads, the
//! program the viewer compiles from them, the clock behind the built-in `time` uniform, and the
//! project file.

pub mod clock;
pub mod program;
pub mod project;
pub mod uniform;

pub use clock::Clock;
pub use program::Program;
pub use project::Project;
pub use uniform::Uniforms;
