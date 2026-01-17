//! Error types for the Markdown converter.

use std::path::PathBuf;

/// Errors that can occur during Markdown conversion.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// I/O error during conversion.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// UTF-8 conversion error.
    #[error("UTF-8 conversion error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),

    /// Output path would be the same as input path.
    #[error("Output path cannot be the same as input path: {0}")]
    OutputPathSameAsInput(PathBuf),

    /// Parser error.
    #[error("Parser error: {0}")]
    Parser(#[from] acdc_parser::Error),
}
