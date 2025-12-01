//! Roff character escaping utilities.
//!
//! This module provides the `manify` function that escapes text for safe inclusion
//! in roff/troff output. It handles special characters, escape sequences, and
//! whitespace normalization.
//!
//! The implementation is equivalent to Asciidoctor's `manify` method but targets
//! modern GNU groff only, avoiding portability conditionals.

use std::borrow::Cow;

/// Escape modes for different content types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EscapeMode {
    /// Normalize whitespace (collapse multiple spaces to one).
    /// Default mode for regular paragraph text.
    #[default]
    Normalize,

    /// Preserve whitespace (for code blocks, literals).
    /// Expands tabs and preserves multiple spaces.
    Preserve,

    /// Collapse all whitespace including wrapped lines.
    /// Used for inline content.
    Collapse,
}

/// Escape text for safe inclusion in roff output.
///
/// This is the main text processing function for manpage output, equivalent
/// to Asciidoctor's `manify` method.
///
/// # Arguments
///
/// * `text` - The text to escape
/// * `mode` - How to handle whitespace
///
/// # Returns
///
/// The escaped text safe for roff output.
#[must_use]
pub fn manify(text: &str, mode: EscapeMode) -> Cow<'_, str> {
    // Fast path: check if any escaping is needed
    if !needs_escaping(text, mode) {
        return Cow::Borrowed(text);
    }

    let mut result = String::with_capacity(text.len() + text.len() / 4);

    // Handle whitespace based on mode
    let processed = match mode {
        EscapeMode::Preserve => {
            // Expand tabs to 8 spaces
            text.replace('\t', "        ")
        }
        EscapeMode::Normalize => {
            // Collapse multiple whitespace to single space
            collapse_whitespace(text)
        }
        EscapeMode::Collapse => {
            // Collapse all whitespace including newlines
            collapse_all_whitespace(text)
        }
    };

    let mut at_line_start = true;

    for ch in processed.chars() {
        match ch {
            // Escape backslash
            '\\' => {
                result.push_str("\\e");
                at_line_start = false;
            }

            // Escape leading period (would be interpreted as macro)
            '.' if at_line_start => {
                result.push_str("\\&.");
                at_line_start = false;
            }

            // Escape leading apostrophe (would be interpreted as macro)
            '\'' if at_line_start => {
                result.push_str("\\&'");
                at_line_start = false;
            }

            // Hyphen/minus - escape to prevent line breaking
            '-' => {
                result.push_str("\\-");
                at_line_start = false;
            }

            // Newline
            '\n' => {
                result.push('\n');
                at_line_start = true;
            }

            // Regular characters
            _ => {
                result.push(ch);
                at_line_start = ch == '\n';
            }
        }
    }

    // Apply special character replacements
    let result = replace_special_chars(&result);

    Cow::Owned(result)
}

/// Check if text needs any escaping.
fn needs_escaping(text: &str, mode: EscapeMode) -> bool {
    // Check for characters that need escaping
    for (i, ch) in text.chars().enumerate() {
        match ch {
            '\\' | '-' => return true,
            '.' | '\'' if i == 0 || text.as_bytes().get(i.saturating_sub(1)) == Some(&b'\n') => {
                return true;
            }
            '\t' if mode == EscapeMode::Preserve => return true,
            _ => {}
        }
    }

    // Check for special Unicode characters
    if text.contains('\u{2014}')
        || text.contains('\u{2013}')
        || text.contains('\u{2018}')
        || text.contains('\u{2019}')
        || text.contains('\u{201C}')
        || text.contains('\u{201D}')
        || text.contains('\u{2026}')
        || text.contains('\u{00A9}')
        || text.contains('\u{00AE}')
        || text.contains('\u{2122}')
        || text.contains('\u{00B0}')
        || text.contains('\u{00A0}')
        || text.contains('\u{2022}')
    {
        return true;
    }

    // Check for whitespace normalization needs
    match mode {
        EscapeMode::Normalize => text.contains("  ") || text.contains('\t'),
        EscapeMode::Collapse => text.contains('\n') || text.contains("  ") || text.contains('\t'),
        EscapeMode::Preserve => false,
    }
}

/// Collapse multiple whitespace characters to single space.
fn collapse_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut prev_whitespace = false;

    for ch in text.chars() {
        if ch.is_ascii_whitespace() && ch != '\n' {
            if !prev_whitespace {
                result.push(' ');
                prev_whitespace = true;
            }
        } else {
            result.push(ch);
            prev_whitespace = false;
        }
    }

    result
}

/// Collapse all whitespace including newlines.
fn collapse_all_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut prev_whitespace = false;

    for ch in text.chars() {
        if ch.is_ascii_whitespace() {
            if !prev_whitespace {
                result.push(' ');
                prev_whitespace = true;
            }
        } else {
            result.push(ch);
            prev_whitespace = false;
        }
    }

    result
}

/// Replace special Unicode characters with roff escape sequences.
fn replace_special_chars(text: &str) -> String {
    text
        // Dashes
        .replace('\u{2014}', "\\(em") // Em dash
        .replace('\u{2013}', "\\(en") // En dash
        // Quotes
        .replace('\u{2018}', "\\(oq") // Left single quote
        .replace('\u{2019}', "\\(cq") // Right single quote / apostrophe
        .replace('\u{201C}', "\\(lq") // Left double quote
        .replace('\u{201D}', "\\(rq") // Right double quote
        // Ellipsis
        .replace('\u{2026}', "...") // Ellipsis
        // Symbols
        .replace('\u{00A9}', "\\(co") // Copyright
        .replace('\u{00AE}', "\\(rg") // Registered
        .replace('\u{2122}', "\\(tm") // Trademark
        .replace('\u{00B0}', "\\(de") // Degree
        .replace('\u{00A0}', "\\ ") // Non-breaking space
        .replace('\u{2022}', "\\(bu") // Bullet
}

/// Escape text for use in double-quoted strings.
///
/// Used for `.TH` arguments and other contexts requiring quoted strings.
#[must_use]
pub fn escape_quoted(text: &str) -> Cow<'_, str> {
    if !text.contains('"') && !text.contains('\\') {
        return Cow::Borrowed(text);
    }

    let result = text.replace('\\', "\\\\").replace('"', "\\\"");

    Cow::Owned(result)
}

/// Convert text to uppercase for section titles.
///
/// Manpage convention is to uppercase level-1 section titles (NAME, SYNOPSIS, etc.).
#[must_use]
pub fn uppercase_title(text: &str) -> String {
    text.to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manify_plain_text() {
        assert_eq!(manify("hello world", EscapeMode::Normalize), "hello world");
    }

    #[test]
    fn test_manify_backslash() {
        assert_eq!(
            manify("path\\to\\file", EscapeMode::Normalize),
            "path\\eto\\efile"
        );
    }

    #[test]
    fn test_manify_hyphen() {
        assert_eq!(manify("--option", EscapeMode::Normalize), "\\-\\-option");
    }

    #[test]
    fn test_manify_leading_period() {
        assert_eq!(manify(".hidden", EscapeMode::Normalize), "\\&.hidden");
    }

    #[test]
    fn test_manify_em_dash() {
        assert_eq!(
            manify("foo\u{2014}bar", EscapeMode::Normalize),
            "foo\\(embar"
        );
    }

    #[test]
    fn test_manify_quotes() {
        assert_eq!(
            manify("\u{201C}quoted\u{201D}", EscapeMode::Normalize),
            "\\(lqquoted\\(rq"
        );
    }

    #[test]
    fn test_manify_preserve_whitespace() {
        assert_eq!(manify("a\tb", EscapeMode::Preserve), "a        b");
    }

    #[test]
    fn test_escape_quoted() {
        assert_eq!(escape_quoted("simple"), "simple");
        assert_eq!(escape_quoted("has \"quotes\""), "has \\\"quotes\\\"");
    }

    #[test]
    fn test_uppercase_title() {
        assert_eq!(uppercase_title("description"), "DESCRIPTION");
        assert_eq!(uppercase_title("See Also"), "SEE ALSO");
    }
}
