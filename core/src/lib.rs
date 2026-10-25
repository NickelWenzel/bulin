//! What bulin edits and runs, free of any GUI or GPU: the uniforms a shader reads, the program
//! the viewer compiles from them, and the clock behind the built-in `time` uniform.

pub mod clock;
pub mod program;
pub mod uniform;

pub use clock::Clock;
pub use program::Program;
pub use uniform::Uniforms;
