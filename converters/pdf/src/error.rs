//! Error types for the PDF converter.

use std::{
    collections::HashMap,
    fmt::Write as _,
    path::{Path, PathBuf},
};

use acdc_parser::Reference;
use acdc_pdf_render::Error as RenderError;

pub(crate) fn with_source_labels(
    error: RenderError,
    references: &HashMap<&str, Reference<'_>>,
    source_file: Option<&Path>,
) -> RenderError {
    match error {
        RenderError::Compile(message) => {
            RenderError::Compile(source_labels(&message, references, source_file))
        }
        RenderError::Pdf(message) => {
            RenderError::Pdf(source_labels(&message, references, source_file))
        }
        error @ RenderError::FontDir { .. } => error,
    }
}

pub(crate) fn source_labels(
    message: &str,
    references: &HashMap<&str, Reference<'_>>,
    source_file: Option<&Path>,
) -> String {
    let mut output = String::with_capacity(message.len());
    let mut rest = message;
    while let Some((prefix, candidate)) = rest.split_once("<id-") {
        let Some((hex, remaining)) = candidate.split_once('>') else {
            break;
        };
        output.push_str(prefix);
        let target = decode_id(hex);
        if let Some((target, reference)) = target
            .as_deref()
            .and_then(|target| references.get(target).map(|reference| (target, reference)))
        {
            let location = &reference.location;
            let file = location
                .start
                .file
                .as_ref()
                .and_then(|chain| chain.last())
                .map(Path::new)
                .or(source_file);
            let file = file.map_or_else(|| "<input>".into(), Path::to_string_lossy);
            let _ = write!(
                output,
                "<{target}> (defined at {file}:{}:{})",
                location.start.line, location.start.column
            );
        } else {
            let _ = write!(output, "<id-{hex}>");
        }
        rest = remaining;
    }
    output.push_str(rest);
    output
}

fn decode_id(hex: &str) -> Option<String> {
    let (pairs, remainder) = hex.as_bytes().as_chunks::<2>();
    if !remainder.is_empty() {
        return None;
    }
    let bytes = pairs
        .iter()
        .map(|[high, low]| {
            let high = char::from(*high).to_digit(16)?;
            let low = char::from(*low).to_digit(16)?;
            u8::try_from(high * 16 + low).ok()
        })
        .collect::<Option<Vec<_>>>()?;
    String::from_utf8(bytes).ok()
}

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

    /// Generated PDF page labels could not be updated.
    #[error("could not update PDF page labels: {0}")]
    PageLabels(#[source] lopdf::Error),
}

fn theme_too_large_message(limit: usize, actual: Option<u64>) -> String {
    match actual {
        Some(actual) => {
            format!("is {actual} bytes, over the maximum allowed size of {limit} bytes")
        }
        None => format!("exceeds the maximum allowed size of {limit} bytes"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_errors_preserve_other_diagnostics_and_describe_source_ids()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse("[[café]]\nText.\n", &acdc_parser::Options::default())?;
        let message = "label `<id-636166c3a9>` occurs multiple times\nunknown variable\nunknown label <id-ff>\nunfinished <id-0";
        let error = with_source_labels(
            RenderError::Compile(message.into()),
            &parsed.document().references,
            Some(Path::new("guide.adoc")),
        );
        assert_eq!(
            error.to_string(),
            "Typst compilation failed:\nlabel `<café> (defined at guide.adoc:1:1)` occurs multiple times\nunknown variable\nunknown label <id-ff>\nunfinished <id-0"
        );
        Ok(())
    }
}
