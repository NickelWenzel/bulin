use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::Error;
use crate::core::library;

#[derive(Debug, Clone)]
pub struct Name(PathBuf);

impl Name {
    pub fn as_str(&self) -> Option<&str> {
        self.0.to_str()
    }

    pub fn file_name(&self) -> Option<&str> {
        self.0.file_name()?.to_str()
    }

    /// Nothing: a file on disk isn't in the browser's library.
    pub fn library_id(&self) -> Option<library::Id> {
        None
    }
}

#[cfg(test)]
impl From<&str> for Name {
    fn from(path: &str) -> Self {
        Self(PathBuf::from(path))
    }
}

pub async fn open_file() -> Result<(Name, Arc<String>), Error> {
    let picked_file = rfd::AsyncFileDialog::new()
        .set_title("Open a text file...")
        .pick_file()
        .await
        .ok_or(Error::DialogClosed)?;

    let path = picked_file.into();

    tokio::fs::read_to_string(&path)
        .await
        .map(|contents| (Name(path), Arc::new(contents)))
        .map_err(Arc::new)
        .map_err(Error::Io)
}

/// Writes `contents` to `path`, or to a file the user picks, suggesting `suggested_name`.
pub async fn save_file(
    path: Option<Name>,
    suggested_name: &str,
    contents: String,
) -> Result<Name, Error> {
    let path = if let Some(path) = path {
        path.0
    } else {
        rfd::AsyncFileDialog::new()
            .set_file_name(suggested_name)
            .save_file()
            .await
            .as_ref()
            .map(rfd::FileHandle::path)
            .map(Path::to_owned)
            .ok_or(Error::DialogClosed)?
    };

    tokio::fs::write(&path, contents)
        .await
        .map(|()| Name(path))
        .map_err(Arc::new)
        .map_err(Error::Io)
}

/// Writes the project `json` to `path`, or to a file the user picks.
pub async fn save_project(path: Option<Name>, json: String) -> Result<Name, Error> {
    save_file(path, "project.bulin", json).await
}

/// Nothing: closing the window asks first through its close request.
pub fn warn_on_unload(_dirty: bool) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_project_writes_to_a_known_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sky.bulin");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();

        let name = runtime
            .block_on(save_project(
                Some(Name::from(path.to_str().unwrap())),
                "{}".to_owned(),
            ))
            .unwrap();

        assert_eq!(name.0, path);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}");
    }
}
