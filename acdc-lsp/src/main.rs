//! acdc-lsp: Language Server Protocol implementation for `AsciiDoc`
//!
//! This binary provides LSP support for `AsciiDoc` documents, enabling features like:
//! - Diagnostics (parse errors shown in editor)
//! - Document symbols (outline of sections)
//! - Go-to-definition (jump from xref to anchor)

use tower_lsp::{LspService, Server};
use tracing_subscriber::EnvFilter;

use acdc_lsp::Backend;

#[tokio::main]
async fn main() {
    // Initialize tracing for debugging - logs go to stderr since stdout is for LSP
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("Starting acdc-lsp server");

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
