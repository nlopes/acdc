//! Shared inputs and observations for converter fixture adapters.

use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use acdc_converters_core::Warning;
use acdc_parser::{Options, ParseResult, SafeMode, parse_file};

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
    Ok(parse_file(path, &options)?)
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
