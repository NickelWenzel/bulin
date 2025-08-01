use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileName(PathBuf);

impl FileName {
    pub fn as_str(&self) -> Option<&str> {
        self.0.to_str()
    }
}

pub async fn open_file() -> Result<(FileName, Arc<String>), Error> {
    let picked_file = rfd::AsyncFileDialog::new()
        .set_title("Open a text file...")
        .pick_file()
        .await
        .ok_or(Error::DialogClosed)?;

    let path = picked_file.into();

    tokio::fs::read_to_string(&path)
        .await
        .map(|contents| (FileName(path), Arc::new(contents)))
        .map_err(Arc::new)
        .map_err(Error::Io)
}

pub async fn save_file(path: Option<FileName>, contents: String) -> Result<FileName, Error> {
    let path = if let Some(path) = path {
        path.0
    } else {
        rfd::AsyncFileDialog::new()
            .save_file()
            .await
            .as_ref()
            .map(rfd::FileHandle::path)
            .map(Path::to_owned)
            .ok_or(Error::DialogClosed)?
    };

    tokio::fs::write(&path, contents)
        .await
        .map(|()| FileName(path))
        .map_err(Arc::new)
        .map_err(Error::Io)
}
