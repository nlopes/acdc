//! Shared syntax highlighting resources and bounded source handling.

use std::sync::OnceLock;

use crate::Diagnostics;

/// Maximum bytes in a physical source line before the entire block is rendered
/// as plain source. Newline separators do not count toward the limit.
///
/// Long lines can cause quadratic tokenization time. Falling back for the whole
/// block preserves multiline syntax state and all source text.
pub const MAX_LINE_BYTES: usize = 16 * 1024;

/// Get the built-in grammar and theme registry shared by all converters.
///
/// Loading also links the built-in grammars. Both successful initialization and
/// a loading error are retained for subsequent calls.
///
/// # Errors
///
/// Returns the loading error if the built-in registry cannot be decoded.
pub fn registry() -> Result<&'static giallo::Registry, &'static giallo::Error> {
    static REGISTRY: OnceLock<Result<giallo::Registry, giallo::Error>> = OnceLock::new();
    REGISTRY.get_or_init(giallo::Registry::builtin).as_ref()
}

/// Whether a block must use plain source to bound highlighting time.
///
/// When diagnostics are supplied, emits at most one line-limit warning per
/// warning source in the document. Call this before loading the registry or
/// tokenizing source. Lines are measured in UTF-8 bytes, with LF, CRLF, and CR
/// treated as separators.
#[must_use]
pub fn skip_highlighting(source: &str, diagnostics: Option<&mut Diagnostics<'_>>) -> bool {
    if !source
        .split(['\r', '\n'])
        .any(|line| line.len() > MAX_LINE_BYTES)
    {
        return false;
    }

    if let Some(diagnostics) = diagnostics {
        let message = format!(
            "source line exceeds the syntax highlighting limit of {MAX_LINE_BYTES} bytes; rendering this block as plain source"
        );
        if !diagnostics
            .warnings()
            .iter()
            .any(|warning| warning.source == *diagnostics.source() && warning.message == message)
        {
            diagnostics.warn_with_advice(
                message,
                "Split long source lines, or disable syntax highlighting for plain output.",
            );
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WarningSource;

    #[test]
    fn line_limit_counts_bytes_and_preserves_physical_line_boundaries() {
        let ascii = "x".repeat(MAX_LINE_BYTES);
        let unicode = "é".repeat(MAX_LINE_BYTES / 2);
        for line in [&ascii, &unicode] {
            assert!(!skip_highlighting(line, None));
            assert!(skip_highlighting(&format!("{line}x"), None));
            for separator in ["\n", "\r\n", "\r"] {
                assert!(!skip_highlighting(
                    &format!("{line}{separator}{line}{separator}"),
                    None,
                ));
            }
        }
    }

    #[test]
    fn line_limit_warning_is_scoped_to_the_document_and_converter() {
        let code = "x".repeat(MAX_LINE_BYTES + 1);
        for _ in 0..2 {
            let mut warnings = Vec::new();
            for converter in ["html", "terminal"] {
                let source = WarningSource::new(converter);
                let mut diagnostics = Diagnostics::new(&source, &mut warnings);
                for _ in 0..2 {
                    assert!(skip_highlighting(&code, Some(&mut diagnostics)));
                }
            }
            assert_eq!(warnings.len(), 2);
            assert!(warnings.iter().all(|warning| warning.advice().is_some()));
        }
    }
}
