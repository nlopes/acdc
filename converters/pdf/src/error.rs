//! Error types for the PDF converter.

use std::path::PathBuf;

/// Errors that can occur during PDF conversion.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// I/O error during conversion.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Output path would be the same as input path.
    #[error("Output path cannot be the same as input path: {0}")]
    OutputPathSameAsInput(PathBuf),

    /// Parser error.
    #[error("Parser error: {0}")]
    Parser(#[from] acdc_parser::Error),

    /// Typst compilation failed.
    #[error("Typst compilation failed: {0}")]
    TypstCompile(String),

    /// PDF export failed.
    #[error("PDF export failed: {0}")]
    PdfExport(String),
}
