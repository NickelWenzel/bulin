//! wgsl-analyzer on a thread of this process.

use std::path::PathBuf;
use std::{env, fs, io, thread};

use matcha_iced::lsp::gen_lsp_types::{self, Uri};
use matcha_iced::lsp::lsp_server::Connection;
use paths::{AbsPathBuf, Utf8PathBuf};
use wgsl_analyzer::config::Config;

use super::files;

/// wgsl-analyzer's own binary gives its threads 16 MiB because salsa's queries recurse deeply.
const STACK_SIZE: usize = 16 << 20;

/// The server runs on its own thread and reads its end of the connection directly, so there is
/// nothing to forward.
pub struct Link {
    failure: Option<String>,
}

impl Link {
    /// Writes the workspace manifest into a scratch directory and starts the server there.
    pub fn start(connection: Connection) -> (Self, Uri) {
        let root = env::temp_dir().join("bulin");
        let uri = Uri::from_directory_path(&root).expect("invariant: the temp dir is absolute");

        let failure = spawn(root, connection).err().map(|error| {
            tracing::error!("wgsl-analyzer could not start: {error}");
            error.to_string()
        });

        (Self { failure }, uri)
    }

    pub fn flush(&mut self) {}

    pub fn failure(&self) -> Option<String> {
        self.failure.clone()
    }
}

/// Dropping `connection` on failure ends the client's connection, so the client reports the
/// server as stopped.
fn spawn(root: PathBuf, connection: Connection) -> io::Result<()> {
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
        })?;

    Ok(())
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
