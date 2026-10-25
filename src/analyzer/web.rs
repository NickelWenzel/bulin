//! The browser has no wgsl-analyzer yet.

use matcha_iced::lsp::gen_lsp_types::Uri;
use matcha_iced::lsp::lsp_server::Connection;

pub struct Link;

impl Link {
    /// Drops the server end at once, so the client reports the server as stopped.
    pub fn start(_connection: Connection) -> (Self, Uri) {
        let root = Uri::parse("file:///workspace/").expect("invariant: a valid URI");
        (Self, root)
    }

    pub fn flush(&mut self) {}
}
