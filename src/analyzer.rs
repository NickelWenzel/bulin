//! wgsl-analyzer, the language server behind the shader editor.
//!
//! The server analyzes one workspace laid out like a folder: its `wesl.toml` makes it a package,
//! since wgsl-analyzer publishes diagnostics only for files inside a discovered package, and
//! depends on the `bulin` and `uniforms` packages under `.bulin/`, each a directory with its own
//! `wesl.toml`. The uniforms package changes with the uniforms, so its text reaches the server as
//! an open document rather than through the file on disk.
//!
//! In a project the workspace is a scratch one bulin writes itself. In a folder it is the folder,
//! and every change bulin applies to its files reaches the server as a watched-file change, since
//! the server doesn't watch on its own. A folder document keeps its path and extension: the server
//! treats a `.wgsl` file as plain WGSL and flags its `import`s, which wesl compiles all the same,
//! so files that import belong in `.wesl` files.

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

use matcha_iced::CodeEditor;
use matcha_iced::lsp::client::{self, Events};
use matcha_iced::lsp::gen_lsp_types::{
    DidChangeWatchedFilesNotification, DidChangeWatchedFilesParams, FileChangeType, FileEvent, Uri,
};
use matcha_iced::lsp::{Client, Update, lsp_server, update};

#[cfg(not(target_arch = "wasm32"))]
use native::Link;
#[cfg(target_arch = "wasm32")]
use web::Link;

use crate::core::folder::{Effect, MANIFEST, Path, generated, is_shader, root_manifest};
use crate::core::program::UNIFORMS_PACKAGE_NAME;
use crate::document::Location;
use crate::folder::Handle;

/// What the server analyzes.
pub enum Workspace {
    /// A scratch directory of bulin's own, for a project.
    Scratch,
    /// The open folder. bulin writes its generated files through the folder, not the analyzer.
    Folder(Handle),
}

/// The files a scratch workspace starts with, relative to its root: the manifest a folder gets and
/// the packages bulin generates in it. The uniforms package's module is empty here: its text is
/// the open document.
fn files() -> Vec<(String, String)> {
    std::iter::once((MANIFEST.to_owned(), root_manifest()))
        .chain(
            generated("")
                .into_iter()
                .map(|(path, text)| (path.as_str().to_owned(), text)),
        )
        .collect()
}

/// The document name of a shader that has no file yet.
const UNTITLED: &str = "untitled.wesl";

/// A running wgsl-analyzer serving one shader editor at a time.
///
/// Every call that talks to the server delivers what it queued before returning, so callers
/// have nothing to flush.
pub struct Analyzer {
    /// Declared before `link`: dropping the client tells the server to exit before the link
    /// lets it go.
    client: Client,
    link: Link,
    root: Uri,
    /// The uniforms package's module, open on the server but never shown.
    uniforms: CodeEditor,
    /// File changes waiting for the server to run, since the client drops notifications before.
    pending: Vec<FileEvent>,
}

impl Analyzer {
    /// Starts the server on `workspace`, returning the stream of its events for
    /// [`Analyzer::receive`].
    ///
    /// Starting never fails: a server that cannot run shows up as a stopped status.
    pub fn start(workspace: Workspace) -> (Self, Events) {
        let (near, far) = lsp_server::Connection::memory();
        let (link, root) = Link::start(far, &workspace);
        let (client, events) = Client::builder().root(root.clone()).memory(near);

        let mut analyzer = Self {
            client,
            link,
            root,
            uniforms: CodeEditor::new(""),
            pending: Vec::new(),
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
        let uri = uri(
            &self.root,
            &format!(".bulin/{UNIFORMS_PACKAGE_NAME}/package.wesl"),
        );
        if let Err(error) = self.uniforms.open_lsp(&mut self.client, &uri, "wesl") {
            tracing::warn!("cannot open {uri} on wgsl-analyzer: {error}");
        }
    }

    /// Tells the server the workspace files changed: each path with what happened to it and, unless
    /// it was removed, its text now. Changes made before the server runs reach it once it does.
    pub fn changed(&mut self, files: &[(Path, Effect, Option<String>)]) {
        for (path, effect, text) in files {
            match (effect, text) {
                (Effect::Removed, _) => self.link.delete_file(path),
                (Effect::Created | Effect::Changed, Some(text)) => self.link.write_file(path, text),
                // Without the text the worker's copy stays as it was; the server still hears of it.
                (Effect::Created | Effect::Changed, None) => {}
            }
            let kind = match effect {
                Effect::Created => FileChangeType::Created,
                Effect::Changed => FileChangeType::Changed,
                Effect::Removed => FileChangeType::Deleted,
            };
            self.pending
                .push(FileEvent::new(uri(&self.root, path.as_str()), kind));
        }
        self.notify();
    }

    /// Sends the file changes waiting for the server, once it runs. A server that stopped never
    /// reads them.
    fn notify(&mut self) {
        match self.client.status() {
            client::Status::Running if !self.pending.is_empty() => {
                let changes = std::mem::take(&mut self.pending);
                self.client.notify::<DidChangeWatchedFilesNotification>(
                    DidChangeWatchedFilesParams { changes },
                );
                self.link.flush();
            }
            client::Status::Running
            | client::Status::Starting
            | client::Status::Restarting { .. } => {}
            client::Status::Stopped(_) => self.pending.clear(),
        }
    }

    pub fn status(&self) -> &client::Status {
        self.client.status()
    }

    /// Why the server could not start or stopped, when this side knows.
    pub fn failure(&self) -> Option<String> {
        self.link.failure()
    }

    /// Starts analyzing `editor`'s text as the shader at `location`. A folder file is analyzed only
    /// when it is a shader of the user's, outside `.bulin/`.
    pub fn attach(&mut self, editor: &mut CodeEditor, location: &Location) {
        let Some(uri) = document_uri(&self.root, location) else {
            return;
        };
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

    /// Tells the server `editor` was saved at `after`, having been at `before`. A new name moves
    /// the document, since the name decides its URI.
    pub fn saved(&mut self, editor: &mut CodeEditor, before: &Location, after: &Location) {
        let uri = document_uri(&self.root, after);
        if uri.is_some() && document_uri(&self.root, before) == uri {
            log_refusal(editor.save_lsp(&mut self.client));
            self.link.flush();
        } else {
            self.moved(editor, before, after);
        }
    }

    /// Reopens `editor`, analyzed at `before`, under the name of `after`. A location that isn't
    /// analyzed, such as a file that isn't a shader, leaves it closed.
    pub fn moved(&mut self, editor: &mut CodeEditor, before: &Location, after: &Location) {
        if document_uri(&self.root, before).is_some() {
            self.detach(editor);
        }
        self.attach(editor, after);
    }

    /// Applies one server event to `editor`; its edits mark the editor dirty like typing does.
    pub fn receive(&mut self, event: client::Event, editor: &mut CodeEditor) {
        for update in self.client.receive(event) {
            match update {
                Update::Document(document) => {
                    log_refusal(editor.apply_lsp(&mut self.client, document));
                }
                Update::Status(status) => {
                    tracing::info!("wgsl-analyzer: {status:?}");
                    if matches!(status, client::Status::Running) {
                        self.notify();
                    }
                }
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
}

/// The URI under which a document at `location` is analyzed in the workspace at `root`, `None` when
/// it isn't analyzed. A project document is named as a `.wesl` module, since the server treats
/// `import` in a `.wgsl` file as an error. A folder file keeps its path, and only the user's shaders
/// are analyzed: `.bulin/uniforms/package.wesl` is already open as the uniforms package.
fn document_uri(root: &Uri, location: &Location) -> Option<Uri> {
    let name = match location {
        Location::Untitled => UNTITLED,
        Location::File(file) => file.file_name().unwrap_or(UNTITLED),
        Location::Folder(path) => {
            return (is_shader(path) && !path.as_str().starts_with(".bulin/"))
                .then(|| uri(root, path.as_str()));
        }
    };
    let stem = name
        .strip_suffix(".wesl")
        .or_else(|| name.strip_suffix(".wgsl"))
        .unwrap_or(name);
    let mut uri = root.clone();
    uri.path_segments_mut()
        .expect("invariant: the workspace root is a file URI")
        .pop_if_empty()
        .push(&format!("{stem}.wesl"));
    Some(uri)
}

/// The URI of the file `path`, relative to the workspace at `root` and separated by `/`.
fn uri(root: &Uri, path: &str) -> Uri {
    let mut uri = root.clone();
    uri.path_segments_mut()
        .expect("invariant: the workspace root is a file URI")
        .pop_if_empty()
        .extend(path.split('/'));
    uri
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
    use std::time::{Duration, Instant};
    use std::{fs, thread};

    use iced::futures::{FutureExt, StreamExt};
    use matcha_core::Severity;
    use matcha_iced::CodeEditor;
    use matcha_iced::lsp::client::{self, Events};
    use tempfile::TempDir;

    use super::{Analyzer, Uri, Workspace};
    use crate::core::folder::{Change, Effect, MANIFEST, Mirror, Path, generated, root_manifest};
    use crate::core::uniform::Type;
    use crate::core::{Program, Uniforms};
    use crate::document::Location;
    use crate::file;
    use crate::folder::Handle;

    fn path(text: &str) -> Path {
        Path::new(text).unwrap()
    }

    /// A folder holding `files` next to the manifest and the packages bulin writes into a folder.
    fn folder(files: &[(&str, &str)]) -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let generated = generated("")
            .map(|(path, text)| (path.as_str().to_owned(), text))
            .into_iter();
        let manifest = (MANIFEST.to_owned(), root_manifest());
        let files = files
            .iter()
            .map(|(file, text)| ((*file).to_owned(), (*text).to_owned()));
        for (file, text) in generated.chain([manifest]).chain(files) {
            let file = dir.path().join(file);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        }
        dir
    }

    /// Starts the analyzer on the folder at `dir` and opens its `main.wesl`, holding `main`.
    fn open(dir: &TempDir, main: &str) -> (Analyzer, Events, CodeEditor) {
        let workspace = Workspace::Folder(Handle::new(dir.path().to_owned()));
        let (mut analyzer, events) = Analyzer::start(workspace);
        let mut editor = CodeEditor::new(main);
        analyzer.attach(&mut editor, &Location::Folder(path("main.wesl")));
        (analyzer, events, editor)
    }

    /// Applies the server's events until `done` holds for `editor`'s error messages, for at most
    /// 30 s, returning them.
    fn settle(
        analyzer: &mut Analyzer,
        events: &mut Events,
        editor: &mut CodeEditor,
        done: impl Fn(&Analyzer, &[String]) -> bool,
    ) -> Vec<String> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let errors = editor
                .document()
                .diagnostics_in(0..u32::MAX)
                .filter(|(_, severity, _)| *severity == Severity::Error)
                .map(|(_, _, message)| message.to_string())
                .collect::<Vec<_>>();
            if done(analyzer, &errors) {
                return errors;
            }
            assert!(Instant::now() < deadline, "still {errors:?} after 30 s");
            match events.next().now_or_never() {
                Some(Some(event)) => analyzer.receive(event, editor),
                Some(None) => panic!("the server stopped"),
                None => thread::sleep(Duration::from_millis(10)),
            }
        }
    }

    const MAIN: &str = "import package::lib::f;\n\
        fn mainBulin(pixel: bulin::Pixel) -> vec4<f32> {\n\
            return vec4<f32>(f());\n\
        }\n";
    /// A main file with one error of its own, so its diagnostics show the imports were resolved.
    const BROKEN_MAIN: &str = "import package::lib::a::f;\n\
        fn mainBulin(pixel: bulin::Pixel) -> vec4<f32> {\n\
            return vec4<f32>(f());\n\
        }\n\
        fn broken() -> i32 { return 1.0; }\n";
    const F: &str = "fn f() -> f32 { return 1.0; }\n";
    const G: &str = "fn g() -> f32 { return 1.0; }\n";

    #[test]
    fn a_folder_workspace_resolves_cross_file_imports() {
        let main = BROKEN_MAIN.replace("lib::a::f", "lib::f");
        for (lib, resolves) in [(F, true), (G, false)] {
            let dir = folder(&[("main.wesl", &main), ("lib.wesl", lib)]);
            let (mut analyzer, mut events, mut editor) = open(&dir, &main);

            let errors = settle(&mut analyzer, &mut events, &mut editor, |_, errors| {
                errors.len() >= if resolves { 1 } else { 2 }
            });
            let unresolved = errors.iter().any(|error| error.contains("import"));
            assert_eq!(unresolved, !resolves, "{lib}: {errors:?}");
            if resolves {
                assert_eq!(errors.len(), 1, "{errors:?}");
            }
        }
    }

    #[test]
    fn disk_changes_reach_the_server() {
        let dir = folder(&[("main.wesl", MAIN), ("lib.wesl", G)]);
        let (mut analyzer, mut events, mut editor) = open(&dir, MAIN);
        settle(&mut analyzer, &mut events, &mut editor, |_, errors| {
            !errors.is_empty()
        });

        fs::write(dir.path().join("lib.wesl"), F).unwrap();
        analyzer.changed(&[(path("lib.wesl"), Effect::Changed, Some(F.to_owned()))]);

        settle(&mut analyzer, &mut events, &mut editor, |_, errors| {
            errors.is_empty()
        });
    }

    #[test]
    fn deleting_a_directory_deletes_its_files() {
        let dir = folder(&[("main.wesl", BROKEN_MAIN), ("lib/a.wesl", F)]);
        let (mut analyzer, mut events, mut editor) = open(&dir, BROKEN_MAIN);
        let errors = settle(&mut analyzer, &mut events, &mut editor, |_, errors| {
            !errors.is_empty()
        });
        assert_eq!(errors.len(), 1, "{errors:?}");

        let mut mirror = Mirror::default();
        let _ = mirror.apply(&Change::Changed {
            path: path("lib/a.wesl"),
            text: Some(F.to_owned()),
        });
        fs::remove_dir_all(dir.path().join("lib")).unwrap();
        let removed = mirror
            .apply(&Change::Removed(path("lib")))
            .into_iter()
            .map(|(path, effect)| (path, effect, None))
            .collect::<Vec<_>>();
        assert_eq!(removed.len(), 1);
        analyzer.changed(&removed);

        settle(&mut analyzer, &mut events, &mut editor, |_, errors| {
            errors.len() >= 2
        });
    }

    #[test]
    fn moving_a_document_reopens_it_under_the_new_uri() {
        let main = BROKEN_MAIN.replace("lib::a::f", "lib::f");
        let dir = folder(&[("main.wgsl", &main), ("lib.wesl", F)]);
        let workspace = Workspace::Folder(Handle::new(dir.path().to_owned()));
        let (mut analyzer, mut events) = Analyzer::start(workspace);
        let mut editor = CodeEditor::new(&main);
        let before = Location::Folder(path("main.wgsl"));
        analyzer.attach(&mut editor, &before);
        // The server flags a `.wgsl` file's imports, on top of its own error.
        settle(&mut analyzer, &mut events, &mut editor, |_, errors| {
            errors.len() >= 2
        });

        fs::rename(dir.path().join("main.wgsl"), dir.path().join("main.wesl")).unwrap();
        analyzer.moved(&mut editor, &before, &Location::Folder(path("main.wesl")));

        let errors = settle(&mut analyzer, &mut events, &mut editor, |_, errors| {
            errors.len() == 1
        });
        assert!(!errors[0].contains("import"), "{errors:?}");
    }

    #[test]
    fn the_folder_workspace_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::Folder(Handle::new(dir.path().to_owned()));
        let (mut analyzer, mut events) = Analyzer::start(workspace);
        let mut editor = CodeEditor::new("");

        settle(&mut analyzer, &mut events, &mut editor, |analyzer, _| {
            matches!(analyzer.status(), client::Status::Running)
        });
        drop(analyzer);

        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn notifications_before_running_arrive_after() {
        let dir = folder(&[("main.wesl", BROKEN_MAIN), ("lib/a.wesl", G)]);
        let (mut analyzer, mut events, mut editor) = open(&dir, BROKEN_MAIN);

        fs::write(dir.path().join("lib/a.wesl"), F).unwrap();
        analyzer.changed(&[(path("lib/a.wesl"), Effect::Changed, Some(F.to_owned()))]);
        assert!(matches!(analyzer.status(), client::Status::Starting));
        assert_eq!(analyzer.pending.len(), 1);

        let errors = settle(
            &mut analyzer,
            &mut events,
            &mut editor,
            |analyzer, errors| analyzer.pending.is_empty() && errors.len() == 1,
        );
        assert!(matches!(analyzer.status(), client::Status::Running));
        assert_eq!(errors.len(), 1, "{errors:?}");
    }

    #[test]
    fn analyzer_reports_a_type_error() {
        let (mut analyzer, mut events) = Analyzer::start(Workspace::Scratch);
        let mut editor = CodeEditor::new("fn f() -> i32 { return 1.0; }");
        analyzer.attach(&mut editor, &Location::Untitled);

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
        let root = Uri::parse("file:///workspace/").unwrap();
        let uri = |file: Option<&str>| {
            let location = file.map_or(Location::Untitled, |file| {
                Location::File(file::Name::from(file))
            });
            super::document_uri(&root, &location)
                .expect("a project document is analyzed")
                .to_string()
        };
        let folder = |file: &str| {
            super::document_uri(&root, &Location::Folder(path(file))).map(|uri| uri.to_string())
        };
        assert_eq!(
            folder("dir/sky.wgsl").as_deref(),
            Some("file:///workspace/dir/sky.wgsl")
        );
        assert_eq!(folder("README.md"), None);
        assert_eq!(folder(".bulin/uniforms/package.wesl"), None);
        let root = root.to_string();

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
        let (mut analyzer, mut events) = Analyzer::start(Workspace::Scratch);
        let mut uniforms = Uniforms::default();
        uniforms.add("radius", Type::Float(1.0)).unwrap();
        analyzer.set_uniforms(&Program::project("", uniforms, None).uniforms_package());
        let mut editor = CodeEditor::new(
            "import uniforms::customs;\n\
             fn mainBulin(pixel: bulin::Pixel) -> vec4<f32> {\n\
                 return vec4<f32>(customs.radius + customs.nope, pixel.uv, 1.0);\n\
             }\n",
        );
        analyzer.attach(&mut editor, &Location::Untitled);

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
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("nope"), "{errors:?}");
    }

    #[test]
    fn diagnostics_follow_uniforms_added_after_the_shader_opened() {
        let (mut analyzer, mut events) = Analyzer::start(Workspace::Scratch);
        let mut editor = CodeEditor::new(
            "import uniforms::customs;\n\
             fn mainBulin(pixel: bulin::Pixel) -> vec4<f32> {\n\
                 return vec4<f32>(customs.radius);\n\
             }\n",
        );
        analyzer.attach(&mut editor, &Location::Untitled);
        let mut errors = |analyzer: &mut Analyzer, editor: &mut CodeEditor, wanted: bool| {
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                let any = editor
                    .document()
                    .diagnostics_in(0..u32::MAX)
                    .any(|(_, severity, _)| severity == Severity::Error);
                if any == wanted {
                    return;
                }
                assert!(Instant::now() < deadline, "errors still {any} after 30 s");
                match events.next().now_or_never() {
                    Some(Some(event)) => analyzer.receive(event, editor),
                    Some(None) => panic!("the server stopped"),
                    None => thread::sleep(Duration::from_millis(10)),
                }
            }
        };

        errors(&mut analyzer, &mut editor, true);
        analyzer.set_uniforms(&Program::project("", radius(), None).uniforms_package());
        errors(&mut analyzer, &mut editor, false);
    }

    fn radius() -> Uniforms {
        let mut uniforms = Uniforms::default();
        uniforms.add("radius", Type::Float(1.0)).unwrap();
        uniforms
    }
}
