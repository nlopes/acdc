//! TCK (Test Compatibility Kit) subcommand for specification compliance testing.
//!
//! This subcommand is used by the asciidoc-tck test harness to validate
//! acdc's parser against the official `AsciiDoc` Language specification.
//!
//! ## Input format (JSON from stdin):
//!
//! ```json
//! {
//!   "contents": "= Document Title\n\nContent here",
//!   "path": "/path/to/input.adoc",
//!   "type": "block"
//! }
//! ```
//!
//! ## Output format:
//!
//! JSON validated against the pinned ASG draft schema. Unsupported constructs
//! return an error before writing output. Passing the TCK does not imply full
//! language conformance.
//!
//! ## Usage:
//!
//! ```bash
//! echo '{"contents":"= Hello","path":"test.adoc","type":"block"}' | acdc tck
//! ```

use std::{
    io::{self, BufReader},
    path::Path,
};

use serde::Deserialize;

use super::json::{self, Error, Format};

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

    let parser_options = acdc_parser::Options::builder()
        .with_compatibility_mode(acdc_parser::CompatibilityMode::Draft)
        .with_safe_mode(acdc_parser::SafeMode::Unsafe)
        .build()?;

    let asg = match tck_input.r#type.as_str() {
        "block" => {
            let parsed = acdc_parser::parse(&tck_input.contents, &parser_options)?;
            json::warnings(parsed.warnings())?;
            json::document(parsed.document(), Format::UpstreamAsg)?
        }
        "inline" => {
            let parsed = acdc_parser::parse_inline(&tck_input.contents, &parser_options)?;
            json::warnings(parsed.warnings())?;
            json::inlines(parsed.inlines(), Format::UpstreamAsg)?
        }
        other => {
            return Err(Error::UnsupportedType(other.to_owned()));
        }
    };
    json::write(&asg, Path::new("-"))
}
