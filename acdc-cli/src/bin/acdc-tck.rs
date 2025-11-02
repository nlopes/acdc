//! TCK (Test Compatibility Kit) binary for spec compliance testing
//!
//! This binary is used by the asciidoc-tck test harness to validate
//! acdc's parser against the official `AsciiDoc` Language specification.
//!
//! ## Input format (JSON from stdin):
//!
//! ```json
//! {
//!   "contents": "= Document Title\n\nContent here",
//!   "path": "/path/to/input.adoc",
//!   "type": "block"  // or "inline"
//! }
//! ```
//!
//! ## Output format:
//!
//! Serialized AST as JSON to stdout, matching the spec's JSON representation.
//!
//! ## Usage:
//!
//! ```bash
//! echo '{"contents":"= Hello","path":"test.adoc","type":"block"}' | acdc-tck
//! ```

use std::io::{self, BufReader, Write};

use acdc_parser::DocumentAttributes;
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    Deserialize(#[from] serde_json::Error),

    #[error(transparent)]
    Io(#[from] io::Error),

    #[error("Parsing error: {0}")]
    Parse(#[from] acdc_parser::Error),
}

#[derive(Debug, Deserialize)]
struct TckInput {
    contents: String,
    path: String,
    r#type: String,
}

fn setup_logging() {
    use tracing_subscriber::{EnvFilter, prelude::*};

    let env_filter = EnvFilter::try_from_env("ACDC_LOG");

    if let Ok(filter) = env_filter {
        let layer = tracing_subscriber::fmt::layer()
            .with_writer(io::stderr)
            .with_ansi(io::IsTerminal::is_terminal(&io::stderr()))
            .with_timer(tracing_subscriber::fmt::time::Uptime::default())
            .with_filter(filter);

        tracing_subscriber::registry().with(layer).init();
    }
}

fn main() -> Result<(), Error> {
    setup_logging();

    // Read JSON input from stdin
    let stdin = io::stdin();
    let reader = BufReader::new(stdin.lock());
    let tck_input: TckInput = serde_json::from_reader(reader)?;

    tracing::debug!(
        path = tck_input.path,
        r#type = tck_input.r#type,
        "processing TCK input",
    );

    // Parse the AsciiDoc content based on type
    let parser_options = acdc_parser::Options {
        safe_mode: acdc_core::SafeMode::Unsafe,
        timings: false,
        document_attributes: DocumentAttributes::default(),
    };

    let mut stdout = io::stdout();
    match tck_input.r#type.as_str() {
        "block" => {
            let doc = acdc_parser::parse(&tck_input.contents, &parser_options)?;
            serde_json::to_writer(&stdout, &doc)?;
        }
        "inline" => {
            let inlines = acdc_parser::parse_inline(&tck_input.contents, &parser_options)?;
            serde_json::to_writer(&stdout, &inlines)?;
        }
        other => {
            eprintln!("Unsupported type: {other}");
            eprintln!("Expected 'block' or 'inline'");
            std::process::exit(1);
        }
    }
    stdout.flush()?;

    Ok(())
}
