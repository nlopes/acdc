//! Error types for the manpage converter.

use std::path::PathBuf;

/// Errors that can occur during manpage conversion.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// I/O error during file operations.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Format error during string formatting.
    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),

    /// Parser error (wrapped for error chain).
    #[error("Parsing error: {0}")]
    Parse(#[from] acdc_parser::Error),

    /// Input and output file paths are the same.
    #[error("input file and output file cannot be the same: {0}")]
    OutputPathSameAsInput(PathBuf),

    /// Invalid manpage title format.
    ///
    /// Manpage titles must follow the format `name(volume)` where:
    /// - `name` is the command/function name
    /// - `volume` is a single digit (1-9) optionally followed by a letter
    #[error("invalid manpage title format: expected 'name(volume)', got '{0}'")]
    InvalidManpageTitle(String),

    /// Missing NAME section.
    ///
    /// Manpage documents require a NAME section as the first section.
    #[error("manpage document missing required NAME section")]
    MissingNameSection,

    /// Invalid NAME section format.
    ///
    /// The NAME section must contain `name - description` format.
    #[error("invalid NAME section format: expected 'name - description', got '{0}'")]
    InvalidNameFormat(String),

    /// Missing document header.
    ///
    /// Manpage documents require a header with the title in `name(volume)` format.
    #[error("manpage document missing required header with title")]
    MissingHeader,

    /// Index out of bounds error.
    #[error("Index out of bounds for {0}: {1}")]
    IndexOutOfBounds(&'static str, usize),
}
