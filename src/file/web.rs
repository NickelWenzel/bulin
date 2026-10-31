use std::cell::{Cell, OnceCell};
use std::io;
use std::sync::Arc;

use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::BeforeUnloadEvent;

use super::Error;
use crate::core::library;
use crate::folder;

/// A file the user opened or saved: a project in the browser's library, or a file uploaded or
/// downloaded under its name.
#[derive(Debug, Clone)]
pub enum Name {
    Library { id: library::Id, name: String },
    Upload(String),
}

impl Name {
    pub fn as_str(&self) -> Option<&str> {
        Some(self.name())
    }

    pub fn file_name(&self) -> Option<&str> {
        Some(self.name())
    }

    /// The library entry holding the project, if the library holds it.
    pub fn library_id(&self) -> Option<library::Id> {
        match self {
            Self::Library { id, .. } => Some(*id),
            Self::Upload(_) => None,
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::Library { name, .. } | Self::Upload(name) => name,
        }
    }
}

pub async fn open_file() -> Result<(Name, Arc<String>), Error> {
    let picked_file = rfd::AsyncFileDialog::new()
        .set_title("Open a text file...")
        .pick_file()
        .await
        .ok_or(Error::DialogClosed)?;

    String::from_utf8(picked_file.read().await)
        .map(|contents| (Name::Upload(picked_file.file_name()), Arc::new(contents)))
        .map_err(Error::ReadFile)
}

/// Downloads `contents` under `filename`, or under a name the user picks, suggesting
/// `suggested_name`.
pub async fn save_file(
    filename: Option<Name>,
    suggested_name: &str,
    contents: String,
) -> Result<Name, Error> {
    let filename = filename.map_or(suggested_name.to_owned(), |f| f.name().to_owned());
    let file_handle = rfd::AsyncFileDialog::new()
        .set_title("Save file...")
        .set_file_name(filename)
        .save_file()
        .await
        .ok_or(Error::DialogClosed)?;

    file_handle
        .write(contents.as_bytes())
        .await
        .map(|()| Name::Upload(file_handle.file_name()))
        .map_err(Arc::new)
        .map_err(Error::Io)
}

/// Stores the project `json` in the browser's library: under its entry when `name` is a library
/// project, otherwise as a new entry named by the user.
pub async fn save_project(name: Option<Name>, json: String) -> Result<Name, Error> {
    let (id, name) = match name {
        Some(Name::Library { id, name }) => (Some(id), name),
        Some(Name::Upload(name)) => (None, ask_name(&name)?),
        None => (None, ask_name("project")?),
    };
    let id = folder::store_project(id, name.clone(), json)
        .await
        .map_err(|error| Error::Io(Arc::new(io::Error::other(error.to_string()))))?;
    Ok(Name::Library { id, name })
}

/// A name for a new library project, suggesting `suggested`, or `DialogClosed` when the user gives
/// none.
fn ask_name(suggested: &str) -> Result<String, Error> {
    let answer = web_sys::window()
        .and_then(|window| {
            window
                .prompt_with_message_and_default("Project name", suggested)
                .ok()
        })
        .flatten();
    answer
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .ok_or(Error::DialogClosed)
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
