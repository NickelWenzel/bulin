//! Where the settings are kept: a file in the config directory on native, localStorage on the
//! web.

#[cfg(not(target_arch = "wasm32"))]
use std::io;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

use crate::core;

/// Why the stored settings couldn't be read.
#[derive(thiserror::Error, Debug, Clone)]
pub enum Error {
    #[cfg(not(target_arch = "wasm32"))]
    #[error("could not read the settings file: {0}")]
    Io(#[source] Arc<io::Error>),
    #[cfg(target_arch = "wasm32")]
    #[error("could not reach the browser's storage: {0}")]
    Storage(String),
    #[error(transparent)]
    Invalid(#[from] core::settings::Error),
}

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::*;

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
