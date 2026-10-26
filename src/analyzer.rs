//! wgsl-analyzer, the language server behind the shader editor.
//!
//! Every document is analyzed as a file in one virtual workspace whose `wesl.toml` makes it a
//! package: wgsl-analyzer publishes diagnostics only for files inside a discovered package. The
//! workspace depends on the `bulin` and `uniforms` packages a shader imports, each a directory
//! with its own `wesl.toml`. The uniforms package changes with the uniforms, so its text reaches
//! the server as an open document rather than through the file on disk.

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

use crate::core::program::{BULIN_PACKAGE_NAME, BULIN_PACKAGE_SOURCE, UNIFORMS_PACKAGE_NAME};
use crate::file;

/// The manifest of a package whose modules sit next to it, instead of in the default `./shaders`.
const PACKAGE_MANIFEST: &str = "edition = \"2026_pre\"\nroot = \".\"\n";

/// The files the workspace starts with, relative to its root: its manifest, depending on the
/// packages bulin provides, and each package as a directory of its own. The uniforms package's
/// module is empty here: its text is the open document.
fn files() -> [(String, String); 5] {
    let uniforms = UNIFORMS_PACKAGE_NAME;
    let bulin = BULIN_PACKAGE_NAME;
    [
        (
            "wesl.toml".to_owned(),
            format!(
                "{PACKAGE_MANIFEST}\n[dependencies]\n\
                 {bulin} = {{ path = \"{bulin}\" }}\n\
                 {uniforms} = {{ path = \"{uniforms}\" }}\n"
            ),
        ),
        (format!("{bulin}/wesl.toml"), PACKAGE_MANIFEST.to_owned()),
        (
            format!("{bulin}/package.wesl"),
            BULIN_PACKAGE_SOURCE.to_owned(),
        ),
        (format!("{uniforms}/wesl.toml"), PACKAGE_MANIFEST.to_owned()),
        (format!("{uniforms}/package.wesl"), String::new()),
    ]
}

/// The document name of a shader that has no file yet.
const UNTITLED: &str = "untitled.wesl";

/// A running wgsl-analyzer serving one shader editor at a time.
///
/// Every call that talks to the server delivers what it queued before returning, so callers
/// have nothing to flush.
pub struct Analyzer {
    client: Client,
    link: Link,
    root: Uri,
    /// The uniforms package's module, open on the server but never shown.
    uniforms: CodeEditor,
}

impl Analyzer {
    /// Starts the server, returning the stream of its events for [`Analyzer::receive`].
    ///
    /// Starting never fails: a server that cannot run shows up as a stopped status.
    pub fn start() -> (Self, Events) {
        let (near, far) = lsp_server::Connection::memory();
        let (link, root) = Link::start(far);
        let (client, events) = Client::builder().root(root.clone()).memory(near);

        let mut analyzer = Self {
            client,
            link,
            root,
            uniforms: CodeEditor::new(""),
        };
        analyzer.open_uniforms();
        analyzer.link.flush();

        (analyzer, events)
    }

    /// Makes `package` the uniforms package shaders import, if it changed.
    pub fn set_uniforms(&mut self, package: &str) {
        if self.uniforms.document().text() == package {
            return;
        }
        self.uniforms.close_lsp(&mut self.client);
        self.uniforms = CodeEditor::new(package);
        self.open_uniforms();
        self.link.flush();
    }

    fn open_uniforms(&mut self) {
        let mut uri = self.root.clone();
        uri.path_segments_mut()
            .expect("invariant: the workspace root is a file URI")
            .pop_if_empty()
            .extend([UNIFORMS_PACKAGE_NAME, "package.wesl"]);
        if let Err(error) = self.uniforms.open_lsp(&mut self.client, &uri, "wesl") {
            tracing::warn!("cannot open {uri} on wgsl-analyzer: {error}");
        }
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
        if let Err(error) = editor.open_lsp(&mut self.client, &uri, "wesl") {
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

    /// The URI under which `file` is analyzed: its name inside the workspace, as a `.wesl`
    /// module, since the server treats `import` in a `.wgsl` file as an error.
    fn document_uri(&self, file: Option<&file::Name>) -> Uri {
        let name = file.and_then(file::Name::file_name).unwrap_or(UNTITLED);
        let stem = name
            .strip_suffix(".wesl")
            .or_else(|| name.strip_suffix(".wgsl"))
            .unwrap_or(name);
        let name = format!("{stem}.wesl");

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
    use crate::core::uniform::Type;
    use crate::core::{Program, Uniforms};
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

        assert_eq!(uri(None), format!("{root}untitled.wesl"));
        assert_eq!(uri(Some("/a/b/sky.wgsl")), format!("{root}sky.wesl"));
        assert_eq!(uri(Some("/a/b/sky.wesl")), format!("{root}sky.wesl"));
        assert_eq!(uri(Some("/a/b/sky.frag")), format!("{root}sky.frag.wesl"));
        assert_eq!(
            uri(Some("/a/b/my sky#1.wgsl")),
            format!("{root}my%20sky%231.wesl")
        );
    }

    #[test]
    fn shaders_import_the_uniforms_and_bulin_packages() {
        let (mut analyzer, mut events) = Analyzer::start();
        let mut uniforms = Uniforms::default();
        uniforms.add("radius", Type::Float(1.0)).unwrap();
        analyzer.set_uniforms(&Program::new("", uniforms, None).uniforms_package());
        let mut editor = CodeEditor::new(
            "import uniforms::customs;\n\
             fn mainBulin(pixel: bulin::Pixel) -> vec4<f32> {\n\
                 return vec4<f32>(customs.radius + customs.nope, pixel.uv, 1.0);\n\
             }\n",
        );
        analyzer.attach(&mut editor, None);

        // The imports resolve, so the only error is the uniform that doesn't exist.
        let deadline = Instant::now() + Duration::from_secs(30);
        let errors = loop {
            assert!(Instant::now() < deadline, "no error diagnostic within 30 s");
            match events.next().now_or_never() {
                Some(Some(event)) => analyzer.receive(event, &mut editor),
                Some(None) => panic!("the server stopped"),
                None => thread::sleep(Duration::from_millis(10)),
            }
            let errors = editor
                .document()
                .diagnostics_in(0..u32::MAX)
                .filter(|(_, severity, _)| *severity == Severity::Error)
                .map(|(_, _, message)| message.to_string())
                .collect::<Vec<_>>();
            if !errors.is_empty() {
                break errors;
            }
        };
        assert_eq!(errors, ["no field `nope` on type ref<Customs>"]);
    }
}
