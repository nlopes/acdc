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
//! echo '{"contents":"= Hello","path":"test.adoc","type":"block"}' | acdc tck
//! ```

use std::io::{self, BufReader, Write};

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
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

/// Run TCK test (no args, reads from stdin)
#[derive(clap::Args)]
pub struct Args;

pub fn run(_args: &Args) -> Result<(), Error> {
    let stdin = io::stdin();
    let reader = BufReader::new(stdin.lock());
    let tck_input: TckInput = serde_json::from_reader(reader)?;

    tracing::debug!(
        path = tck_input.path,
        r#type = tck_input.r#type,
        "processing TCK input",
    );

    // Parse the AsciiDoc content based on type
    let parser_options = acdc_parser::Options::builder()
        .with_safe_mode(acdc_parser::SafeMode::Unsafe)
        .build();

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
            tracing::error!(type=other, "Unsupported type, expected 'block' or 'inline'");
            std::process::exit(1);
        }
    }
    stdout.flush()?;

    Ok(())
}
