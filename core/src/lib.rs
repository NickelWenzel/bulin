//! What bulin edits, runs and saves, free of any GUI or GPU: the uniforms a shader reads, the
//! program the viewer compiles from them, the clock behind the built-in `time` uniform, the
//! project file, folders and how the web stores, imports and exports them, and the settings the
//! editor keeps across projects.

pub mod archive;
pub mod clock;
pub mod color;
pub mod folder;
pub mod library;
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
