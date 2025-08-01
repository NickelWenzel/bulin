use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileName(String);

impl FileName {
    pub fn as_str(&self) -> Option<&str> {
        Some(&self.0)
    }
}

pub async fn open_file() -> Result<(FileName, Arc<String>), Error> {
    let picked_file = rfd::AsyncFileDialog::new()
        .set_title("Open a text file...")
        .pick_file()
        .await
        .ok_or(Error::DialogClosed)?;

    String::from_utf8(picked_file.read().await)
        .map(|contents| (FileName(picked_file.file_name()), Arc::new(contents)))
        .map_err(Error::ReadFile)
}

pub async fn save_file(_filename: Option<FileName>, contents: String) -> Result<FileName, Error> {
    let file_handle = rfd::AsyncFileDialog::new()
        .set_title("Save file...")
        .save_file()
        .await
        .ok_or(Error::DialogClosed)?;

    file_handle
        .write(contents.as_bytes())
        .await
        .map(|()| FileName(file_handle.file_name()))
        .map_err(Arc::new)
        .map_err(Error::Io)
}
