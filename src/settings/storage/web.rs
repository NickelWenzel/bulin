use super::Error;
use crate::core;

/// The settings in `localStorage["bulin.settings"]`, or `None` when nothing is stored there.
pub fn load() -> Result<Option<core::Settings>, Error> {
    let storage = web_sys::window()
        .ok_or_else(|| Error::Storage("no window".to_owned()))?
        .local_storage()
        .map_err(|error| Error::Storage(format!("{error:?}")))?
        .ok_or_else(|| Error::Storage("no localStorage".to_owned()))?;

    let json = storage
        .get_item("bulin.settings")
        .map_err(|error| Error::Storage(format!("{error:?}")))?;

    Ok(json
        .map(|json| core::Settings::from_json(&json))
        .transpose()?)
}
