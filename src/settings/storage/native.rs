use std::io;
use std::sync::Arc;

use super::Error;
use crate::core;

/// The settings in `settings.json` under the user's config directory, or `None` when there is
/// no such file.
pub fn load() -> Result<Option<core::Settings>, Error> {
    let Some(directory) = dirs::config_dir() else {
        return Ok(None);
    };

    match std::fs::read_to_string(directory.join("bulin").join("settings.json")) {
        Ok(json) => Ok(Some(core::Settings::from_json(&json)?)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::Io(Arc::new(error))),
    }
}
