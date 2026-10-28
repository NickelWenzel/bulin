use super::Error;
use crate::core;

/// The settings in `localStorage["bulin.settings"]`, or `None` when nothing is stored there.
pub fn load() -> Result<Option<core::Settings>, Error> {
    let json = storage()?
        .get_item("bulin.settings")
        .map_err(|error| Error::Storage(format!("{error:?}")))?;

    Ok(json
        .map(|json| core::Settings::from_json(&json))
        .transpose()?)
}

/// Writes `settings` to `localStorage["bulin.settings"]`.
pub async fn save(settings: core::Settings) -> Result<(), Error> {
    storage()?
        .set_item("bulin.settings", &settings.to_json())
        .map_err(|error| Error::Storage(format!("{error:?}")))
}

/// Copies `localStorage["bulin.settings"]` to `bulin.settings.bak`, so writing new settings
/// keeps the old.
pub fn back_up() -> Result<(), Error> {
    let storage = storage()?;
    let json = storage
        .get_item("bulin.settings")
        .map_err(|error| Error::Storage(format!("{error:?}")))?;

    match json {
        Some(json) => storage
            .set_item("bulin.settings.bak", &json)
            .map_err(|error| Error::Storage(format!("{error:?}"))),
        None => Ok(()),
    }
}

fn storage() -> Result<web_sys::Storage, Error> {
    web_sys::window()
        .ok_or_else(|| Error::Storage("no window".to_owned()))?
        .local_storage()
        .map_err(|error| Error::Storage(format!("{error:?}")))?
        .ok_or_else(|| Error::Storage("no localStorage".to_owned()))
}
