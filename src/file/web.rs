use std::cell::{Cell, OnceCell};
use std::sync::Arc;

use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::BeforeUnloadEvent;

use super::Error;

#[derive(Debug, Clone)]
pub struct Name(String);

impl Name {
    pub fn as_str(&self) -> Option<&str> {
        Some(&self.0)
    }

    pub fn file_name(&self) -> Option<&str> {
        Some(&self.0)
    }
}

pub async fn open_file() -> Result<(Name, Arc<String>), Error> {
    let picked_file = rfd::AsyncFileDialog::new()
        .set_title("Open a text file...")
        .pick_file()
        .await
        .ok_or(Error::DialogClosed)?;

    String::from_utf8(picked_file.read().await)
        .map(|contents| (Name(picked_file.file_name()), Arc::new(contents)))
        .map_err(Error::ReadFile)
}

/// Downloads `contents` under `filename`, or under a name the user picks, suggesting
/// `suggested_name`.
pub async fn save_file(
    filename: Option<Name>,
    suggested_name: &str,
    contents: String,
) -> Result<Name, Error> {
    let filename = filename.map_or(suggested_name.to_owned(), |f| f.0);
    let file_handle = rfd::AsyncFileDialog::new()
        .set_title("Save file...")
        .set_file_name(filename)
        .save_file()
        .await
        .ok_or(Error::DialogClosed)?;

    file_handle
        .write(contents.as_bytes())
        .await
        .map(|()| Name(file_handle.file_name()))
        .map_err(Arc::new)
        .map_err(Error::Io)
}

thread_local! {
    /// Whether leaving the page would lose unsaved changes.
    static DIRTY: Cell<bool> = const { Cell::new(false) };
    /// The page's `beforeunload` listener, kept for as long as the page.
    static LISTENER: OnceCell<Closure<dyn FnMut(BeforeUnloadEvent)>> = const { OnceCell::new() };
}

/// Has the browser warn before the page closes or reloads while `dirty`.
pub fn warn_on_unload(dirty: bool) {
    DIRTY.set(dirty);
    LISTENER.with(|listener| {
        listener.get_or_init(|| {
            let listener =
                Closure::<dyn FnMut(BeforeUnloadEvent)>::new(|event: BeforeUnloadEvent| {
                    if DIRTY.get() {
                        event.prevent_default();
                        // Some browsers warn only when the event has a return value.
                        event.set_return_value("");
                    }
                });
            let added = web_sys::window().map(|window| {
                window.add_event_listener_with_callback(
                    "beforeunload",
                    listener.as_ref().unchecked_ref(),
                )
            });
            if let Some(Err(error)) = added {
                tracing::warn!("the page can't warn before it closes: {error:?}");
            }
            listener
        });
    });
}
