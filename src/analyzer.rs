//! wgsl-analyzer, the language server behind the shader editor.
//!
//! Every document is analyzed as a file in one virtual workspace whose `wesl.toml` makes it a
//! package: wgsl-analyzer publishes diagnostics only for files inside a discovered package.

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

use matcha_iced::lsp::client::{self, Events};
use matcha_iced::lsp::gen_lsp_types::Uri;
use matcha_iced::lsp::{Client, lsp_server};

#[cfg(not(target_arch = "wasm32"))]
use native::Link;
#[cfg(target_arch = "wasm32")]
use web::Link;

use crate::util::FileName;

/// The document name of a shader that has no file yet.
const UNTITLED: &str = "untitled.wgsl";

/// A running wgsl-analyzer and the client connected to it.
pub struct Analyzer {
    client: Client,
    link: Link,
    root: Uri,
}

impl Analyzer {
    /// Starts the server, returning the stream the host feeds back through
    /// [`Client::receive`].
    ///
    /// Starting never fails: a server that cannot run shows up as a stopped client.
    pub fn start() -> (Self, Events) {
        let (near, far) = lsp_server::Connection::memory();
        let (link, root) = Link::start(far);
        let (client, events) = Client::builder().root(root.clone()).memory(near);

        let mut analyzer = Self { client, link, root };
        analyzer.flush();

        (analyzer, events)
    }

    pub fn client(&mut self) -> &mut Client {
        &mut self.client
    }

    pub fn status(&self) -> &client::Status {
        self.client.status()
    }

    /// Hands the server whatever the client queued since the last call.
    ///
    /// The host calls this after every update because a server in a web worker can only be
    /// reached from the UI thread.
    pub fn flush(&mut self) {
        self.link.flush();
    }

    /// The URI under which `file` is analyzed: its name inside the workspace, with a `.wgsl`
    /// extension added when it has neither `.wgsl` nor `.wesl`.
    pub fn document_uri(&self, file: Option<&FileName>) -> Uri {
        let name = file.and_then(FileName::file_name).unwrap_or(UNTITLED);
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

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::{Duration, Instant};

    use iced::futures::{FutureExt, StreamExt};
    use matcha_core::Severity;
    use matcha_iced::CodeEditor;
    use matcha_iced::lsp::{Update, update};

    use super::Analyzer;
    use crate::util::FileName;

    #[test]
    fn analyzer_reports_a_type_error() {
        let (mut analyzer, mut events) = Analyzer::start();
        let mut editor = CodeEditor::new("fn f() -> i32 { return 1.0; }");
        let uri = analyzer.document_uri(None);
        editor
            .open_lsp(analyzer.client(), &uri, "wgsl")
            .expect("the document opens");

        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            assert!(Instant::now() < deadline, "no error diagnostic within 30 s");
            analyzer.flush();

            match events.next().now_or_never() {
                Some(Some(event)) => {
                    for update in analyzer.client().receive(event) {
                        let Update::Document(document) = update else {
                            continue;
                        };
                        if let update::Change::Diagnostics(diagnostics) = document.change()
                            && diagnostics.iter().any(|d| d.severity == Severity::Error)
                        {
                            return;
                        }
                        let _ = editor.apply_lsp(analyzer.client(), document);
                    }
                }
                Some(None) => panic!("the server stopped"),
                None => thread::sleep(Duration::from_millis(10)),
            }
        }
    }

    #[test]
    fn documents_are_named_after_their_file_inside_the_workspace() {
        let (analyzer, _events) = Analyzer::start();
        let uri = |file: Option<&str>| {
            let file = file.map(FileName::from);
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
