//! wgsl-analyzer, the language server behind the shader editor.
//!
//! Every document is analyzed as a file in one virtual workspace whose `wesl.toml` makes it a
//! package: wgsl-analyzer publishes diagnostics only for files inside a discovered package.

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

use matcha_iced::CodeEditor;
use matcha_iced::lsp::client::{self, Events};
use matcha_iced::lsp::gen_lsp_types::Uri;
use matcha_iced::lsp::{Client, Update, lsp_server, update};

#[cfg(not(target_arch = "wasm32"))]
use native::Link;
#[cfg(target_arch = "wasm32")]
use web::Link;

use crate::file;

/// The workspace manifest. `root = "."` keeps the documents at the workspace root instead of
/// the default `./shaders`.
const MANIFEST: &str = "edition = \"2026_pre\"\nroot = \".\"\n";

/// The document name of a shader that has no file yet.
const UNTITLED: &str = "untitled.wgsl";

/// A running wgsl-analyzer serving one shader editor at a time.
///
/// Every call that talks to the server delivers what it queued before returning, so callers
/// have nothing to flush.
pub struct Analyzer {
    client: Client,
    link: Link,
    root: Uri,
}

impl Analyzer {
    /// Starts the server, returning the stream of its events for [`Analyzer::receive`].
    ///
    /// Starting never fails: a server that cannot run shows up as a stopped status.
    pub fn start() -> (Self, Events) {
        let (near, far) = lsp_server::Connection::memory();
        let (link, root) = Link::start(far);
        let (client, events) = Client::builder().root(root.clone()).memory(near);

        let mut analyzer = Self { client, link, root };
        analyzer.link.flush();

        (analyzer, events)
    }

    pub fn status(&self) -> &client::Status {
        self.client.status()
    }

    /// Why the server could not start or stopped, when this side knows.
    pub fn failure(&self) -> Option<String> {
        self.link.failure()
    }

    /// Starts analyzing `editor`'s text as the shader `file`.
    pub fn attach(&mut self, editor: &mut CodeEditor, file: Option<&file::Name>) {
        let uri = self.document_uri(file);
        if let Err(error) = editor.open_lsp(&mut self.client, &uri, "wgsl") {
            tracing::warn!("cannot open {uri} on wgsl-analyzer: {error}");
        }
        self.link.flush();
    }

    /// Stops analyzing `editor`, clearing what the server reported for it.
    pub fn detach(&mut self, editor: &mut CodeEditor) {
        editor.close_lsp(&mut self.client);
        self.link.flush();
    }

    /// Sends `editor`'s edits and pending requests, such as hover or completion.
    /// Call it after every update of the editor.
    pub fn sync(&mut self, editor: &mut CodeEditor) {
        log_refusal(editor.sync_lsp(&mut self.client));
        self.link.flush();
    }

    /// Tells the server `editor` was saved as `after`, having been `before`. A new name moves
    /// the document, since the name decides its URI.
    pub fn saved(
        &mut self,
        editor: &mut CodeEditor,
        before: Option<&file::Name>,
        after: &file::Name,
    ) {
        if self.document_uri(before) == self.document_uri(Some(after)) {
            log_refusal(editor.save_lsp(&mut self.client));
            self.link.flush();
        } else {
            self.detach(editor);
            self.attach(editor, Some(after));
        }
    }

    /// Applies one server event to `editor`; its edits mark the editor dirty like typing does.
    pub fn receive(&mut self, event: client::Event, editor: &mut CodeEditor) {
        for update in self.client.receive(event) {
            match update {
                Update::Document(document) => {
                    log_refusal(editor.apply_lsp(&mut self.client, document));
                }
                Update::Status(status) => tracing::info!("wgsl-analyzer: {status:?}"),
                Update::Log(entry) if entry.is_shown() => {
                    tracing::info!("wgsl-analyzer: {}", entry.text())
                }
                Update::Log(entry) => tracing::debug!("wgsl-analyzer: {}", entry.text()),
                Update::Error(error) => tracing::warn!("wgsl-analyzer: {error}"),
                // Renames only reach the open shader, so edits for other files never come.
                Update::FileEdits(_) | Update::Notification(_) | Update::Trace(_) => {}
            }
        }
        self.link.flush();
    }

    /// The URI under which `file` is analyzed: its name inside the workspace, with a `.wgsl`
    /// extension added when it has neither `.wgsl` nor `.wesl`.
    fn document_uri(&self, file: Option<&file::Name>) -> Uri {
        let name = file.and_then(file::Name::file_name).unwrap_or(UNTITLED);
        let name = if name.ends_with(".wgsl") || name.ends_with(".wesl") {
            name.to_owned()
        } else {
            format!("{name}.wgsl")
        };

        let mut uri = self.root.clone();
        uri.path_segments_mut()
            .expect("invariant: the workspace root is a file URI")
            .pop_if_empty()
            .push(&name);
        uri
    }
}

/// Logs an update the editor refused. Its jump, a definition in another document, is dropped:
/// the editor holds one shader.
fn log_refusal(applied: update::Applied) {
    if let Some(refusal) = applied.refused {
        tracing::debug!("wgsl-analyzer update not applied: {refusal}");
    }
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::{Duration, Instant};

    use iced::futures::{FutureExt, StreamExt};
    use matcha_core::Severity;
    use matcha_iced::CodeEditor;

    use super::Analyzer;
    use crate::file;

    #[test]
    fn analyzer_reports_a_type_error() {
        let (mut analyzer, mut events) = Analyzer::start();
        let mut editor = CodeEditor::new("fn f() -> i32 { return 1.0; }");
        analyzer.attach(&mut editor, None);

        let deadline = Instant::now() + Duration::from_secs(30);
        while !editor
            .document()
            .diagnostics_in(0..u32::MAX)
            .any(|(_, severity, _)| severity == Severity::Error)
        {
            assert!(Instant::now() < deadline, "no error diagnostic within 30 s");

            match events.next().now_or_never() {
                Some(Some(event)) => analyzer.receive(event, &mut editor),
                Some(None) => panic!("the server stopped"),
                None => thread::sleep(Duration::from_millis(10)),
            }
        }
    }

    #[test]
    fn documents_are_named_after_their_file_inside_the_workspace() {
        let (analyzer, _events) = Analyzer::start();
        let uri = |file: Option<&str>| {
            let file = file.map(file::Name::from);
            analyzer.document_uri(file.as_ref()).to_string()
        };
        let root = analyzer.root.to_string();

        assert_eq!(uri(None), format!("{root}untitled.wgsl"));
        assert_eq!(uri(Some("/a/b/sky.wgsl")), format!("{root}sky.wgsl"));
        assert_eq!(uri(Some("/a/b/sky.wesl")), format!("{root}sky.wesl"));
        assert_eq!(uri(Some("/a/b/sky.frag")), format!("{root}sky.frag.wgsl"));
        assert_eq!(
            uri(Some("/a/b/my sky#1.wgsl")),
            format!("{root}my%20sky%231.wgsl")
        );
    }
}
