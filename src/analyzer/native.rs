//! wgsl-analyzer on a thread of this process.

use std::thread::{self, JoinHandle};
use std::{env, fs, io};

use matcha_iced::lsp::gen_lsp_types::{self, Uri};
use matcha_iced::lsp::lsp_server::Connection;
use paths::{AbsPathBuf, Utf8PathBuf};
use tempfile::TempDir;
use wgsl_analyzer::config::Config;

use super::files;

/// wgsl-analyzer's own binary gives its threads 16 MiB because salsa's queries recurse deeply.
const STACK_SIZE: usize = 16 << 20;

/// The server runs on its own thread and reads its end of the connection directly, so there is
/// nothing to forward.
pub struct Link {
    failure: Option<String>,
    server: Option<JoinHandle<()>>,
    /// Removed once the server has stopped reading it.
    workspace: Option<TempDir>,
}

impl Link {
    /// Writes the workspace into a scratch directory of its own and starts the server there.
    pub fn start(connection: Connection) -> (Self, Uri) {
        let workspace = tempfile::Builder::new().prefix("bulin-").tempdir();
        let root = workspace
            .as_ref()
            .map_or_else(|_| env::temp_dir(), |workspace| workspace.path().to_owned());
        let uri = Uri::from_directory_path(&root).expect("invariant: the temp dir is absolute");

        let mut link = Self {
            failure: None,
            server: None,
            workspace: None,
        };
        match workspace.and_then(|workspace| Ok((spawn(&workspace, connection)?, workspace))) {
            Ok((server, workspace)) => {
                link.server = Some(server);
                link.workspace = Some(workspace);
            }
            Err(error) => {
                tracing::error!("wgsl-analyzer could not start: {error}");
                link.failure = Some(error.to_string());
            }
        }

        (link, uri)
    }

    pub fn flush(&mut self) {}

    pub fn failure(&self) -> Option<String> {
        self.failure.clone()
    }
}

impl Drop for Link {
    /// The client, dropped before the link, has told the server to exit; waiting for it keeps the
    /// workspace in place until the server stops reading it.
    fn drop(&mut self) {
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

/// Dropping `connection` on failure ends the client's connection, so the client reports the
/// server as stopped.
fn spawn(workspace: &TempDir, connection: Connection) -> io::Result<JoinHandle<()>> {
    let root = workspace.path().to_owned();
    for (path, contents) in files() {
        let path = root.join(path);
        if let Some(directory) = path.parent() {
            fs::create_dir_all(directory)?;
        }
        fs::write(path, contents)?;
    }
    let root = Utf8PathBuf::from_path_buf(root)
        .ok()
        .and_then(|root| AbsPathBuf::try_from(root).ok())
        .ok_or_else(|| io::Error::other("the temp dir is not an absolute UTF-8 path"))?;

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
