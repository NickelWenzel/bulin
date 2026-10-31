//! wgsl-analyzer on a thread of this process.

use std::path::PathBuf;
use std::thread::{self, JoinHandle};
use std::{env, fs, io};

use matcha_iced::lsp::gen_lsp_types::{self, Uri};
use matcha_iced::lsp::lsp_server::Connection;
use paths::{AbsPathBuf, Utf8PathBuf};
use tempfile::TempDir;
use wgsl_analyzer::config::Config;

use super::{Workspace, files};
use crate::core::folder::Path;

/// wgsl-analyzer's own binary gives its threads 16 MiB because salsa's queries recurse deeply.
const STACK_SIZE: usize = 16 << 20;

/// The server runs on its own thread and reads its end of the connection directly, so there is
/// nothing to forward. It reads the workspace from disk, so a change on disk needs no copy.
pub struct Link {
    failure: Option<String>,
    server: Option<JoinHandle<()>>,
    /// A scratch workspace, removed once the server has stopped reading it.
    scratch: Option<TempDir>,
}

impl Link {
    /// Starts the server on `workspace`: a folder in place, or a scratch directory of its own with
    /// the files a project needs.
    pub fn start(connection: Connection, workspace: &Workspace) -> (Self, Uri) {
        let workspace = match workspace {
            Workspace::Scratch => tempfile::Builder::new()
                .prefix("bulin-")
                .tempdir()
                .and_then(|scratch| {
                    for (path, contents) in files() {
                        let path = scratch.path().join(path);
                        if let Some(directory) = path.parent() {
                            fs::create_dir_all(directory)?;
                        }
                        fs::write(path, contents)?;
                    }
                    Ok((scratch.path().to_owned(), Some(scratch)))
                }),
            Workspace::Folder(handle) => {
                std::path::absolute(handle.root()).map(|root| (root, None))
            }
        };
        let root = workspace
            .as_ref()
            .map_or_else(|_| env::temp_dir(), |(root, _)| root.clone());
        let uri =
            Uri::from_directory_path(&root).expect("invariant: the workspace root is absolute");

        let mut link = Self {
            failure: None,
            server: None,
            scratch: None,
        };
        match workspace.and_then(|(root, scratch)| Ok((spawn(root, connection)?, scratch))) {
            Ok((server, scratch)) => {
                link.server = Some(server);
                link.scratch = scratch;
            }
            Err(error) => {
                tracing::error!("wgsl-analyzer could not start: {error}");
                link.failure = Some(error.to_string());
            }
        }

        (link, uri)
    }

    pub fn flush(&mut self) {}

    pub fn write_file(&mut self, _path: &Path, _text: &str) {}

    pub fn delete_file(&mut self, _path: &Path) {}

    pub fn failure(&self) -> Option<String> {
        self.failure.clone()
    }
}

impl Drop for Link {
    /// The client, dropped before the link, has told the server to exit. A thread of its own waits
    /// for it, so the UI never does, and removes a scratch workspace once the server stops reading
    /// it.
    fn drop(&mut self) {
        let server = self.server.take();
        let scratch = self.scratch.take();
        let spawned = thread::Builder::new()
            .name("wgsl-analyzer-exit".to_owned())
            .spawn(move || {
                if let Some(server) = server {
                    let _ = server.join();
                }
                drop(scratch);
            });
        if let Err(error) = spawned {
            tracing::warn!("cannot wait for wgsl-analyzer to exit: {error}");
        }
    }
}

/// Dropping `connection` on failure ends the client's connection, so the client reports the
/// server as stopped.
fn spawn(root: PathBuf, connection: Connection) -> io::Result<JoinHandle<()>> {
    let root = Utf8PathBuf::from_path_buf(root)
        .ok()
        .and_then(|root| AbsPathBuf::try_from(root).ok())
        .ok_or_else(|| io::Error::other("the workspace is not an absolute UTF-8 path"))?;

    thread::Builder::new()
        .name("wgsl-analyzer".to_owned())
        .stack_size(STACK_SIZE)
        .spawn(move || {
            if let Err(error) = run(root, connection) {
                tracing::error!("wgsl-analyzer stopped: {error:#}");
            }
        })
}

/// The initialize handshake of wgsl-analyzer's `session::run_session`, then its main loop until
/// the client sends `exit`.
fn run(root: AbsPathBuf, connection: Connection) -> anyhow::Result<()> {
    // Fails only when the global pool already exists, which is as good.
    let _ = rayon::ThreadPoolBuilder::new()
        .thread_name(|index| format!("wgsl-analyzer-rayon-{index}"))
        .stack_size(STACK_SIZE)
        .build_global();

    let (id, params) = connection.initialize_start()?;
    let gen_lsp_types::InitializeParams {
        capabilities,
        client_info,
        ..
    } = wgsl_analyzer::from_json("InitializeParams", &params)?;

    let config = Config::new(root.clone(), capabilities, vec![root], client_info);
    let result = gen_lsp_types::InitializeResult {
        capabilities: wgsl_analyzer::server_capabilities(&config),
        server_info: Some(gen_lsp_types::ServerInfo {
            name: "wgsl-analyzer".to_owned(),
            version: Some(wgsl_analyzer::version().to_string()),
        }),
    };
    connection.initialize_finish(id, serde_json::to_value(result)?)?;

    wgsl_analyzer::main_loop(config, connection)
}
