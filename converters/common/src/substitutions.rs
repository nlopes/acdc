//! Text substitution utilities for `AsciiDoc` converters.
//!
//! This module provides functions for processing `AsciiDoc` text substitutions
//! that are common across different output formats (HTML, terminal, etc.).

// Private Use Area placeholders for escaped patterns.
// These characters won't appear in normal text and are used to protect
// escaped patterns from typography substitutions.
const ESCAPED_ELLIPSIS: &str = "\u{E000}ELLIPSIS\u{E000}";
const ESCAPED_ARROW_RIGHT: &str = "\u{E000}RARROW\u{E000}";
const ESCAPED_ARROW_LEFT: &str = "\u{E000}LARROW\u{E000}";
const ESCAPED_DARROW_RIGHT: &str = "\u{E000}RDARROW\u{E000}";
const ESCAPED_DARROW_LEFT: &str = "\u{E000}LDARROW\u{E000}";
const ESCAPED_EMDASH: &str = "\u{E000}EMDASH\u{E000}";

/// Remove backslash escapes from `AsciiDoc` formatting characters and patterns.
///
/// Converts escape sequences like `\^` → `^`, `\~` → `~`, `\\` → `\`, etc.
/// Also handles multi-character pattern escapes like `\...`, `\->`, `\--`.
/// This should only be applied to non-verbatim content - verbatim contexts
/// (monospace, source blocks, literal blocks) should preserve backslashes.
///
/// # Supported escape sequences
///
/// ## Single characters
/// - `\*` → `*` (bold marker)
/// - `\_` → `_` (italic marker)
/// - `` \` `` → `` ` `` (monospace marker)
/// - `\#` → `#` (highlight marker)
/// - `\^` → `^` (superscript marker)
/// - `\~` → `~` (subscript marker)
/// - `\\` → `\` (literal backslash)
/// - `\[` → `[` (attribute/macro opener)
/// - `\]` → `]` (attribute/macro closer)
///
/// ## Multi-character patterns (converted to placeholders)
/// - `\...` → placeholder (prevents ellipsis conversion)
/// - `\->` → placeholder (prevents right arrow conversion)
/// - `\<-` → placeholder (prevents left arrow conversion)
/// - `\=>` → placeholder (prevents right double arrow conversion)
/// - `\<=` → placeholder (prevents left double arrow conversion)
/// - `\--` → placeholder (prevents em-dash conversion)
///
/// Call [`restore_escaped_patterns`] after typography substitutions to convert
/// placeholders back to their literal forms.
///
/// # Example
///
/// ```
/// use acdc_converters_common::substitutions::strip_backslash_escapes;
///
/// assert_eq!(strip_backslash_escapes(r"E=mc\^2"), "E=mc^2");
/// assert_eq!(strip_backslash_escapes(r"H\~2~O"), "H~2~O");
/// // Note: \\ is preserved when not followed by escapable syntax (matching asciidoctor)
/// assert_eq!(strip_backslash_escapes(r"path\\to\\file"), r"path\\to\\file");
/// // But \\ followed by escapable char is handled by the parser, not here
/// ```
#[must_use]
pub fn strip_backslash_escapes(text: &str) -> String {
    // First, replace multi-character pattern escapes with placeholders.
    // This protects them from typography substitutions.
    let text = text
        .replace("\\...", ESCAPED_ELLIPSIS)
        .replace("\\->", ESCAPED_ARROW_RIGHT)
        .replace("\\<-", ESCAPED_ARROW_LEFT)
        .replace("\\=>", ESCAPED_DARROW_RIGHT)
        .replace("\\<=", ESCAPED_DARROW_LEFT)
        .replace("\\--", ESCAPED_EMDASH);

    // Then handle single-character escapes
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        // Handle single-character escapes (excluding backslash itself).
        // Note: \\ is NOT stripped here. Per asciidoctor behavior:
        // - \\ alone or followed by non-escapable text -> preserved as \\
        // - \\** (double backslash + double marker) is handled by the parser
        //   which produces just ** in the AST, so we never see \\** here
        if c == '\\'
            && chars
                .peek()
                .is_some_and(|&next| matches!(next, '*' | '_' | '`' | '#' | '^' | '~' | '[' | ']'))
        {
            // \x -> x (skip backslash, output the character)
            if let Some(escaped) = chars.next() {
                result.push(escaped);
                continue;
            }
        }
        result.push(c);
    }
    result
}

/// Restore escaped patterns after typography substitutions are complete.
///
/// This converts the placeholders created by [`strip_backslash_escapes`] back
/// to their literal forms. Call this after applying typography substitutions
/// (ellipsis, arrows, em-dash) to preserve the escaped patterns.
///
/// # Example
///
/// ```
/// use acdc_converters_common::substitutions::{strip_backslash_escapes, restore_escaped_patterns};
///
/// let input = r"v2.0.25\...v2.0.26";
/// let escaped = strip_backslash_escapes(input);
/// // Typography substitutions would happen here...
/// let restored = restore_escaped_patterns(&escaped);
/// assert_eq!(restored, "v2.0.25...v2.0.26");
/// ```
#[must_use]
pub fn restore_escaped_patterns(text: &str) -> String {
    text.replace(ESCAPED_ELLIPSIS, "...")
        .replace(ESCAPED_ARROW_RIGHT, "->")
        .replace(ESCAPED_ARROW_LEFT, "<-")
        .replace(ESCAPED_DARROW_RIGHT, "=>")
        .replace(ESCAPED_DARROW_LEFT, "<=")
        .replace(ESCAPED_EMDASH, "--")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_caret_escape() {
        assert_eq!(strip_backslash_escapes(r"\^"), "^");
        assert_eq!(strip_backslash_escapes(r"E=mc\^2"), "E=mc^2");
        assert_eq!(strip_backslash_escapes(r"\^not super^"), "^not super^");
    }

    #[test]
    fn test_strip_tilde_escape() {
        assert_eq!(strip_backslash_escapes(r"\~"), "~");
        assert_eq!(strip_backslash_escapes(r"H\~2~O"), "H~2~O");
        assert_eq!(strip_backslash_escapes(r"\~not sub~"), "~not sub~");
    }

    #[test]
    fn test_strip_other_escapes() {
        assert_eq!(strip_backslash_escapes(r"\*bold\*"), "*bold*");
        assert_eq!(strip_backslash_escapes(r"\_italic\_"), "_italic_");
        assert_eq!(strip_backslash_escapes(r"\`mono\`"), "`mono`");
        assert_eq!(strip_backslash_escapes(r"\#marked\#"), "#marked#");
        // Note: \\ is now preserved per asciidoctor behavior (double backslash
        // escaping is handled by the parser, not the converter)
        assert_eq!(strip_backslash_escapes(r"\\"), r"\\");
        assert_eq!(strip_backslash_escapes(r"\[attr\]"), "[attr]");
    }

    #[test]
    fn test_preserves_other_backslashes() {
        // Backslashes not followed by escapable chars are preserved
        assert_eq!(strip_backslash_escapes(r"\n"), r"\n");
        assert_eq!(strip_backslash_escapes(r"C:\path"), r"C:\path");
        // Single dot is NOT escapable (backslash preserved)
        assert_eq!(strip_backslash_escapes(r"a\.b"), r"a\.b");
    }

    #[test]
    fn test_empty_and_no_escapes() {
        assert_eq!(strip_backslash_escapes(""), "");
        assert_eq!(strip_backslash_escapes("plain text"), "plain text");
    }

    #[test]
    fn test_strip_pattern_escapes() {
        // Ellipsis escape - uses placeholder
        assert_eq!(strip_backslash_escapes(r"\..."), ESCAPED_ELLIPSIS);
        assert!(strip_backslash_escapes(r"v2.0.25\...v2.0.26").contains(ESCAPED_ELLIPSIS));

        // Arrow escapes - use placeholders
        assert_eq!(strip_backslash_escapes(r"\->"), ESCAPED_ARROW_RIGHT);
        assert_eq!(strip_backslash_escapes(r"\<-"), ESCAPED_ARROW_LEFT);
        assert_eq!(strip_backslash_escapes(r"\=>"), ESCAPED_DARROW_RIGHT);
        assert_eq!(strip_backslash_escapes(r"\<="), ESCAPED_DARROW_LEFT);
        assert_eq!(strip_backslash_escapes(r"\--"), ESCAPED_EMDASH);
    }

    #[test]
    fn test_restore_escaped_patterns() {
        assert_eq!(restore_escaped_patterns(ESCAPED_ELLIPSIS), "...");
        assert_eq!(restore_escaped_patterns(ESCAPED_ARROW_RIGHT), "->");
        assert_eq!(restore_escaped_patterns(ESCAPED_ARROW_LEFT), "<-");
        assert_eq!(restore_escaped_patterns(ESCAPED_DARROW_RIGHT), "=>");
        assert_eq!(restore_escaped_patterns(ESCAPED_DARROW_LEFT), "<=");
        assert_eq!(restore_escaped_patterns(ESCAPED_EMDASH), "--");
    }

    #[test]
    fn test_roundtrip_escape_restore() {
        let input = r"v2.0.25\...v2.0.26";
        let escaped = strip_backslash_escapes(input);
        let restored = restore_escaped_patterns(&escaped);
        assert_eq!(restored, "v2.0.25...v2.0.26");
    }

    #[test]
    fn test_roundtrip_arrows() {
        // Test that escaped arrows survive the roundtrip
        assert_eq!(
            restore_escaped_patterns(&strip_backslash_escapes(r"use \-> instead")),
            "use -> instead"
        );
        assert_eq!(
            restore_escaped_patterns(&strip_backslash_escapes(r"\<- back")),
            "<- back"
        );
    }
}
