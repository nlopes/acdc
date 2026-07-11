use std::collections::HashSet;

use typst::diag::SourceDiagnostic;
use typst_as_lib::TypstAsLibError;

/// An error produced while compiling markup to a PDF.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The Typst source failed to compile. The message lists the diagnostics
    /// against the generated `.typ` (use `--emit-typst` to inspect it).
    #[error("Typst compilation failed:\n{0}")]
    Compile(String),
    /// A font directory entry or font file could not be read.
    #[error("could not read font path {path}: {source}")]
    FontDir {
        path: String,
        source: std::io::Error,
    },
    /// PDF export failed after a successful compile.
    #[error("PDF export failed:\n{0}")]
    Pdf(String),
}

impl From<TypstAsLibError> for Error {
    fn from(err: TypstAsLibError) -> Self {
        match err {
            TypstAsLibError::TypstSource(diags) => Error::Compile(format_diagnostics(&diags)),
            other @ (TypstAsLibError::TypstFile(_)
            | TypstAsLibError::MainSourceFileDoesNotExist(_)
            | TypstAsLibError::HintedString(_)
            | TypstAsLibError::Unspecified(_)) => Error::Compile(other.to_string()),
        }
    }
}

/// Render Typst diagnostics into a readable, de-duplicated block. Line/column
/// references point at the generated markup, not the original Markdown.
pub(crate) fn format_diagnostics(diagnostics: &[SourceDiagnostic]) -> String {
    let mut seen = HashSet::new();
    let mut lines = Vec::new();
    for diagnostic in diagnostics {
        let message = diagnostic.message.as_str();
        if seen.insert(message.to_owned()) {
            lines.push(format!("  - {message}"));
        }
    }
    if lines.is_empty() {
        "  (no diagnostics reported)".to_owned()
    } else {
        lines.join("\n")
    }
}
