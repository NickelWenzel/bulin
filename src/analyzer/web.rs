//! wgsl-analyzer-web, the server compiled to emscripten, in a Web Worker driven by
//! `web/analyzer.js`.
//!
//! The worker's messages go straight into the connection from a callback. What the client sends
//! waits in the connection until [`Link::flush`] hands it to the worker. The worker has a
//! filesystem of its own, so every change to the workspace's files is copied there.

use std::cell::RefCell;
use std::rc::Rc;

use matcha_iced::lsp::gen_lsp_types::Uri;
use matcha_iced::lsp::lsp_server::{Connection, Message};
use wasm_bindgen::prelude::{Closure, wasm_bindgen};

use super::{Workspace, files};
use crate::core::folder::{MANIFEST, Path, generated, root_manifest};
use crate::folder::{Watched, scan};

#[wasm_bindgen(module = "/web/analyzer.js")]
extern "C" {
    fn analyzer_begin() -> u32;
    fn analyzer_start(
        generation: u32,
        root: &str,
        files_json: &str,
        on_message: &Closure<dyn FnMut(String)>,
        on_failed: &Closure<dyn FnMut(String)>,
    );
    fn analyzer_send(json: &str);
    fn analyzer_write_file(path: &str, text: &str);
    fn analyzer_delete_file(path: &str);
    fn analyzer_stop(generation: u32);
}

/// The workspace root in the worker's in-memory filesystem.
const ROOT: &str = "/workspace";

/// The worker's callbacks, kept alive while a folder's files are read before it starts.
struct Callbacks {
    on_message: Closure<dyn FnMut(String)>,
    on_failed: Closure<dyn FnMut(String)>,
}

pub struct Link {
    /// Numbers this link's server in `analyzer.js`, so a dropped link never starts or stops
    /// another's.
    generation: u32,
    /// Dropped when the worker fails, which ends the client's connection.
    connection: Rc<RefCell<Option<Connection>>>,
    failure: Rc<RefCell<Option<String>>>,
    _callbacks: Rc<Callbacks>,
}

impl Link {
    /// Starts loading the worker on `workspace`: a scratch workspace right away, a folder once its
    /// files are read from the library. What the client sends meanwhile waits in `analyzer.js`.
    pub fn start(connection: Connection, workspace: &Workspace) -> (Self, Uri) {
        let generation = analyzer_begin();
        let connection = Rc::new(RefCell::new(Some(connection)));
        let failure = Rc::new(RefCell::new(None));

        let on_message = Closure::new({
            let connection = Rc::clone(&connection);
            move |json: String| {
                let Some(connection) = &*connection.borrow() else {
                    return;
                };
                match serde_json::from_str::<Message>(&json) {
                    Ok(message) => {
                        let _ = connection.sender.send(message);
                    }
                    Err(error) => tracing::warn!("undecodable message from wgsl-analyzer: {error}"),
                }
            }
        });
        let on_failed = Closure::new({
            let connection = Rc::clone(&connection);
            let failure = Rc::clone(&failure);
            move |reason: String| {
                tracing::warn!("wgsl-analyzer is unavailable: {reason}");
                *failure.borrow_mut() = Some(reason);
                connection.borrow_mut().take();
            }
        });
        let callbacks = Rc::new(Callbacks {
            on_message,
            on_failed,
        });

        match workspace {
            Workspace::Scratch => start(generation, files(), &callbacks),
            Workspace::Folder(handle) => {
                let handle = handle.clone();
                let callbacks = Rc::clone(&callbacks);
                wasm_bindgen_futures::spawn_local(async move {
                    let mut seed = match scan(&handle).await {
                        Ok(Watched::Scan(scan)) => scan
                            .into_iter()
                            .filter_map(|(path, text)| Some((path.as_str().to_owned(), text?)))
                            .collect(),
                        Ok(Watched::Gone) => Vec::new(),
                        Err(error) => {
                            tracing::warn!(
                                "cannot read {} for wgsl-analyzer: {error}",
                                handle.name()
                            );
                            Vec::new()
                        }
                    };
                    // The folder gets these files as the analyzer starts, and they reach the worker
                    // again once written.
                    if !seed.iter().any(|(path, _)| path == MANIFEST) {
                        seed.push((MANIFEST.to_owned(), root_manifest()));
                    }
                    seed.extend(
                        generated("")
                            .into_iter()
                            .map(|(path, text)| (path.as_str().to_owned(), text)),
                    );
                    start(generation, seed, &callbacks);
                });
            }
        }

        let root = Uri::parse(&format!("file://{ROOT}/")).expect("invariant: a valid URI");
        let link = Self {
            generation,
            connection,
            failure,
            _callbacks: callbacks,
        };
        (link, root)
    }

    pub fn flush(&mut self) {
        let Some(connection) = &*self.connection.borrow() else {
            return;
        };
        for message in connection.receiver.try_iter() {
            match serde_json::to_string(&message) {
                Ok(json) => analyzer_send(&json),
                Err(error) => tracing::warn!("cannot encode a message for wgsl-analyzer: {error}"),
            }
        }
    }

    /// Copies `text` to the file `path` of the worker's workspace.
    pub fn write_file(&mut self, path: &Path, text: &str) {
        analyzer_write_file(path.as_str(), text);
    }

    /// Removes the file `path` from the worker's workspace.
    pub fn delete_file(&mut self, path: &Path) {
        analyzer_delete_file(path.as_str());
    }

    pub fn failure(&self) -> Option<String> {
        self.failure.borrow().clone()
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        analyzer_stop(self.generation);
    }
}

/// Starts the `generation`th server on a workspace holding `files`, by their paths relative to its
/// root. `analyzer.js` ignores a generation that was stopped or superseded.
fn start(generation: u32, files: Vec<(String, String)>, callbacks: &Callbacks) {
    let files = serde_json::Value::Object(
        files
            .into_iter()
            .map(|(path, contents)| (path, contents.into()))
            .collect(),
    )
    .to_string();
    analyzer_start(
        generation,
        ROOT,
        &files,
        &callbacks.on_message,
        &callbacks.on_failed,
    );
}
