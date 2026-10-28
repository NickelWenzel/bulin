use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use super::Error;
use crate::core;

/// The settings in `settings.json` under the user's config directory, or `None` when there is
/// no such file.
pub fn load() -> Result<Option<core::Settings>, Error> {
    let Some(directory) = directory() else {
        return Ok(None);
    };

    match std::fs::read_to_string(directory.join("settings.json")) {
        Ok(json) => Ok(Some(core::Settings::from_json(&json)?)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::Io(Arc::new(error))),
    }
}

/// Writes `settings` to `settings.json`, creating the directory. The file is replaced in one
/// step, so a write cut short leaves the previous settings in place.
pub async fn save(settings: core::Settings) -> Result<(), Error> {
    let Some(directory) = directory() else {
        return Ok(());
    };

    let temporary = directory.join("settings.json.tmp");
    async {
        tokio::fs::create_dir_all(&directory).await?;
        tokio::fs::write(&temporary, settings.to_json()).await?;
        tokio::fs::rename(&temporary, directory.join("settings.json")).await
    }
    .await
    .map_err(|error| Error::Io(Arc::new(error)))
}

/// Moves `settings.json` aside to `settings.json.bak`, so writing new settings keeps the old.
pub fn back_up() -> Result<(), Error> {
    let Some(directory) = directory() else {
        return Ok(());
    };

    match std::fs::rename(
        directory.join("settings.json"),
        directory.join("settings.json.bak"),
    ) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(Error::Io(Arc::new(error))),
        _ => Ok(()),
    }
}

fn directory() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("bulin"))
}
