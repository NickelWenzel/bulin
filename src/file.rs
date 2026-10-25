use std::io;
use std::string::FromUtf8Error;
use std::sync::Arc;

#[derive(thiserror::Error, Debug, Clone)]
pub enum Error {
    #[error("Dialog was closed")]
    DialogClosed,
    #[error("Could not read file content")]
    ReadFile(#[from] FromUtf8Error),
    #[error("I/O error occurred")]
    Io(#[source] Arc<io::Error>),
}

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::*;

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
