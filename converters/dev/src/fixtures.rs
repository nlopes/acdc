//! Shared inputs and observations for converter fixture adapters.

use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use acdc_converters_core::Warning;
use acdc_parser::{Options, ParseResult, SafeMode, parse_file};

mod display;

/// Output and structured diagnostics produced by a fixture adapter.
pub struct FixtureOutput {
    /// Rendered bytes, before test-specific normalization.
    pub output: Vec<u8>,
    /// Converter warnings, retained for semantic assertions.
    pub warnings: Vec<Warning>,
}

/// Parse a reference fixture with the same include access as the CLI.
///
/// # Errors
/// Returns an error when parser options or the input are invalid.
pub fn parse_fixture(path: &Path) -> Result<ParseResult, Box<dyn Error>> {
    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .build()?;
    parse_fixture_with_options(path, &options)
}

/// Parse a fixture with caller-supplied backend defaults and generic fixture processors.
///
/// # Errors
/// Returns an error when parsing or a fixture's extension request fails.
pub fn parse_fixture_with_options(
    path: &Path,
    options: &Options,
) -> Result<ParseResult, Box<dyn Error>> {
    // Keep ordinary fixtures on the ordinary parser path. Only the display
    // fixture family opts into this public extension consumer.
    let display_fixture = path
        .file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.strip_prefix("commonmark_")
                .unwrap_or(name)
                .starts_with("extension_display")
        });
    if display_fixture {
        Ok(display::registry()?.parse_file(path, options)?.finish()?)
    } else {
        Ok(parse_file(path, options)?)
    }
}

/// Discover sorted `.adoc` files directly inside a fixture source directory.
/// Supporting inputs in `includes/` and other subdirectories are not cases.
///
/// # Errors
/// Returns an error when the directory or an entry cannot be read.
pub fn source_files(directory: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "adoc")
            && fs::metadata(&path)?.is_file()
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}
