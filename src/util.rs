use std::path::PathBuf;
use std::sync::Arc;

use rfd::FileHandle;

#[derive(Debug, Clone)]
pub enum Error {
    DialogClosed,
    IoError,
}

pub async fn open_file() -> Result<(PathBuf, Arc<String>), Error> {
    let picked_file = rfd::AsyncFileDialog::new()
        .set_title("Open a text file...")
        .pick_file()
        .await
        .ok_or(Error::DialogClosed)?;

    load_file(picked_file).await
}

pub async fn load_file(filehandle: FileHandle) -> Result<(PathBuf, Arc<String>), Error> {
    let contents = String::from_utf8(filehandle.read().await)
        .map(Arc::new)
        .map_err(|_| Error::IoError)?;

    Ok((filehandle.file_name().into(), contents))
}

pub async fn save_file(path: Option<PathBuf>, contents: String) -> Result<PathBuf, Error> {
    // let path = if let Some(path) = path {
    //     FileHandle(path)
    // } else {
    //     rfd::AsyncFileDialog::new()
    //         .save_file()
    //         .await
    //         .ok_or(Error::DialogClosed)?
    // };

    let file_handle = rfd::AsyncFileDialog::new()
        .save_file()
        .await
        .ok_or(Error::DialogClosed)?;

    file_handle
        .write(&contents.into_bytes())
        .await
        .map_err(|_| Error::IoError)?;

    Ok(path.unwrap_or(file_handle.file_name().into()))
}
