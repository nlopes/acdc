//! Error types for the PDF converter.

use std::path::PathBuf;

use acdc_pdf_render::Error as RenderError;

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

    /// Theme file could not be read.
    #[error("could not read PDF theme {path}: {source}")]
    ThemeRead {
        /// Theme path.
        path: PathBuf,
        /// I/O error.
        source: std::io::Error,
    },

    /// Theme file exceeds the supported input limit.
    #[error(
        "PDF theme {path} {message}",
        message = theme_too_large_message(*limit, *actual)
    )]
    ThemeTooLarge {
        /// Theme path.
        path: PathBuf,
        /// Maximum supported byte length.
        limit: usize,
        /// Exact size when it was available before reading.
        actual: Option<u64>,
    },

    /// Theme file could not be parsed.
    #[error("could not parse PDF theme {path}: {source}")]
    ThemeParse {
        /// Theme path.
        path: PathBuf,
        /// Parse error.
        source: acdc_pdf_theme::Error,
    },

    /// Image or logo resolution failed in strict mode.
    #[error("{0}")]
    AssetResolution(String),

    /// Debug Typst output could not be written.
    #[error("could not write Typst markup to {path}: {source}")]
    TypstWrite {
        /// Output path.
        path: PathBuf,
        /// I/O error.
        source: std::io::Error,
    },

    /// Typst rendering or PDF export failed.
    #[error(transparent)]
    Render(#[from] RenderError),
}

fn theme_too_large_message(limit: usize, actual: Option<u64>) -> String {
    match actual {
        Some(actual) => {
            format!("is {actual} bytes, over the maximum allowed size of {limit} bytes")
        }
        None => format!("exceeds the maximum allowed size of {limit} bytes"),
    }
}
