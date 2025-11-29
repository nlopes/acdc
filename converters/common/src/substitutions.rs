//! Text substitution utilities for `AsciiDoc` converters.
//!
//! This module provides functions for processing `AsciiDoc` text substitutions
//! that are common across different output formats (HTML, terminal, etc.).

/// Remove backslash escapes from `AsciiDoc` formatting characters.
///
/// Converts escape sequences like `\^` → `^`, `\~` → `~`, `\\` → `\`, etc.
/// This should only be applied to non-verbatim content - verbatim contexts
/// (monospace, source blocks, literal blocks) should preserve backslashes.
///
/// # Supported escape sequences
///
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
/// # Example
///
/// ```
/// use acdc_converters_common::substitutions::strip_backslash_escapes;
///
/// assert_eq!(strip_backslash_escapes(r"E=mc\^2"), "E=mc^2");
/// assert_eq!(strip_backslash_escapes(r"H\~2~O"), "H~2~O");
/// assert_eq!(strip_backslash_escapes(r"path\\to\\file"), r"path\to\file");
/// ```
#[must_use]
pub fn strip_backslash_escapes(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\'
            && let Some(&next) = chars.peek()
            && matches!(next, '*' | '_' | '`' | '#' | '^' | '~' | '\\' | '[' | ']')
        {
            // Skip the backslash, output the next character
            if let Some(escaped) = chars.next() {
                result.push(escaped);
                continue;
            }
        }
        result.push(c);
    }
    result
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
        assert_eq!(strip_backslash_escapes(r"\\"), r"\");
        assert_eq!(strip_backslash_escapes(r"\[attr\]"), "[attr]");
    }

    #[test]
    fn test_preserves_other_backslashes() {
        // Backslashes not followed by escapable chars are preserved
        assert_eq!(strip_backslash_escapes(r"\n"), r"\n");
        assert_eq!(strip_backslash_escapes(r"C:\path"), r"C:\path");
    }

    #[test]
    fn test_empty_and_no_escapes() {
        assert_eq!(strip_backslash_escapes(""), "");
        assert_eq!(strip_backslash_escapes("plain text"), "plain text");
    }
}
