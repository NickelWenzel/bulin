//! wgsl-analyzer-web, the server compiled to emscripten, in a Web Worker driven by
//! `web/analyzer.js`.
//!
//! The worker's messages go straight into the connection from a callback. What the client sends
//! waits in the connection until [`Link::flush`] hands it to the worker.

use std::cell::RefCell;
use std::rc::Rc;

use matcha_iced::lsp::gen_lsp_types::Uri;
use matcha_iced::lsp::lsp_server::{Connection, Message};
use wasm_bindgen::prelude::{Closure, wasm_bindgen};

use super::FILES;

#[wasm_bindgen(module = "/web/analyzer.js")]
extern "C" {
    fn analyzer_start(
        root: &str,
        files_json: &str,
        on_message: &Closure<dyn FnMut(String)>,
        on_failed: &Closure<dyn FnMut(String)>,
    );
    fn analyzer_send(json: &str);
    fn analyzer_stop();
}

/// The workspace root in the worker's in-memory filesystem.
const ROOT: &str = "/workspace";

pub struct Link {
    /// Dropped when the worker fails, which ends the client's connection.
    connection: Rc<RefCell<Option<Connection>>>,
    failure: Rc<RefCell<Option<String>>>,
    _on_message: Closure<dyn FnMut(String)>,
    _on_failed: Closure<dyn FnMut(String)>,
}

impl Link {
    /// Starts loading the worker, seeding the workspace with its manifest.
    pub fn start(connection: Connection) -> (Self, Uri) {
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

        let files = serde_json::Value::Object(
            FILES
                .iter()
                .map(|(path, contents)| ((*path).to_owned(), (*contents).into()))
                .collect(),
        )
        .to_string();
        analyzer_start(ROOT, &files, &on_message, &on_failed);

        let root = Uri::parse(&format!("file://{ROOT}/")).expect("invariant: a valid URI");
        let link = Self {
            connection,
            failure,
            _on_message: on_message,
            _on_failed: on_failed,
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

    pub fn failure(&self) -> Option<String> {
        self.failure.borrow().clone()
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        analyzer_stop();
    }
}
