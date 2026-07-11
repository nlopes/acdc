//! Escaping of text for Typst markup — the highest-correctness-risk module.
//!
//! Two contexts, two functions:
//! - [`escape_markup`] for literal text placed inside a content block `[…]`.
//! - [`escape_string`] for text placed inside a Typst string literal `"…"`.

use std::fmt::Write as _;

/// Characters that are always significant in Typst markup and so must be
/// backslash-escaped wherever they appear in literal text.
fn is_always_special(ch: char) -> bool {
    matches!(
        ch,
        '\\' | '#' | '$' | '*' | '_' | '`' | '<' | '>' | '@' | '~' | '[' | ']'
    )
}

/// Escape literal text for inclusion in a Typst content block.
///
/// `at_line_start` must be `true` only when the text begins at the very start of
/// a content block / paragraph / heading / cell / list item, where a leading
/// `-`, `+`, `=`, `/`, or `N.` / `N)` would otherwise be parsed as block markup.
/// Embedded newlines always begin a new line regardless of this initial value.
pub(crate) fn escape_markup(out: &mut String, text: &str, at_line_start: bool) {
    let chars: Vec<char> = text.chars().collect();
    let mut line_start = at_line_start;
    let mut ordered_punct = if line_start {
        ordered_marker_index(&chars)
    } else {
        None
    };

    for (i, &ch) in chars.iter().enumerate() {
        let next = chars.get(i + 1).copied();
        let escape = if is_always_special(ch) {
            true
        } else if ch == '/' {
            // `//` and `/*` start comments anywhere; a leading `/` starts a term
            // list.
            line_start || matches!(next, Some('/' | '*'))
        } else if matches!(ch, '-' | '+' | '=') {
            line_start
        } else {
            Some(i) == ordered_punct
        };

        if escape {
            out.push('\\');
        }
        out.push(ch);

        line_start = matches!(ch, '\n' | '\r');
        if line_start {
            ordered_punct = chars
                .get(i + 1..)
                .and_then(ordered_marker_index)
                .map(|offset| i + 1 + offset);
        }
    }
}

/// If the leading run is `[0-9]+` followed by `.` or `)` (an ordered-list
/// marker), return the char index of that punctuation so it can be escaped.
fn ordered_marker_index(chars: &[char]) -> Option<usize> {
    let mut saw_digit = false;
    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_ascii_digit() {
            saw_digit = true;
        } else if saw_digit && matches!(ch, '.' | ')') {
            return Some(i);
        } else {
            return None;
        }
    }
    None
}

/// Escape text for inclusion inside a Typst string literal (`"…"`), used for
/// paths, raw code content, and language tags.
pub(crate) fn escape_string(out: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{{{:x}}}", c as u32);
            }
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn markup(text: &str, at_line_start: bool) -> String {
        let mut out = String::new();
        escape_markup(&mut out, text, at_line_start);
        out
    }

    fn string(text: &str) -> String {
        let mut out = String::new();
        escape_string(&mut out, text);
        out
    }

    #[test]
    fn always_special_chars_are_escaped_anywhere() {
        for ch in ['\\', '#', '$', '*', '_', '`', '<', '>', '@', '~', '[', ']'] {
            let input = format!("a{ch}b");
            let expected = format!("a\\{ch}b");
            assert_eq!(markup(&input, false), expected, "char {ch:?}");
        }
    }

    #[test]
    fn backslash_is_escaped_first_not_doubled() {
        // A single backslash becomes exactly two, not four.
        assert_eq!(markup("\\", false), "\\\\");
    }

    #[test]
    fn line_leading_block_markers_escaped_only_at_start() {
        assert_eq!(markup("- item", true), "\\- item");
        assert_eq!(markup("+ item", true), "\\+ item");
        assert_eq!(markup("= head", true), "\\= head");
        // Not at line start: left alone.
        assert_eq!(markup("a - b", false), "a - b");
        assert_eq!(markup("a - b", true), "a - b");
    }

    #[test]
    fn ordered_list_marker_escaped_at_line_start() {
        assert_eq!(markup("1. first", true), "1\\. first");
        assert_eq!(markup("42) item", true), "42\\) item");
        // Mid-text or not at start: untouched.
        assert_eq!(markup("see 1. here", true), "see 1. here");
        assert_eq!(markup("1. first", false), "1. first");
    }

    #[test]
    fn block_markers_are_escaped_after_embedded_newlines() {
        assert_eq!(
            markup("intro\n- item\r\n42) item\n/ term", false),
            "intro\n\\- item\r\n42\\) item\n\\/ term"
        );
    }

    #[test]
    fn slash_comments_are_neutralised_anywhere() {
        assert_eq!(markup("a // b", false), "a \\// b");
        // Both the slash (comment) and the star (always special) are escaped.
        assert_eq!(markup("a /* b", false), "a \\/\\* b");
        // A lone slash mid-text is fine.
        assert_eq!(markup("and/or", false), "and/or");
        // Leading slash at line start is escaped.
        assert_eq!(markup("/path", true), "\\/path");
    }

    #[test]
    fn unicode_and_quotes_pass_through_markup() {
        // Smart quotes disabled downstream; unicode is a font concern, not an
        // escaping one.
        assert_eq!(
            markup("\u{201c}hi\u{201d} \u{2014} ok", false),
            "\u{201c}hi\u{201d} \u{2014} ok"
        );
    }

    #[test]
    fn string_literal_escapes() {
        assert_eq!(string("a\"b\\c"), "a\\\"b\\\\c");
        assert_eq!(string("line1\nline2\t."), "line1\\nline2\\t.");
        assert_eq!(string("nul\u{0}here"), "nul\\u{0}here");
        assert_eq!(string("del\u{7f}c1\u{85}"), "del\\u{7f}c1\\u{85}");
        // Backticks and markup chars are NOT special inside a string literal.
        assert_eq!(string("`#*_"), "`#*_");
    }

    #[test]
    fn string_literal_output_contains_no_raw_control_characters() {
        let controls = (0..=0x9f)
            .filter_map(char::from_u32)
            .filter(|ch| ch.is_control())
            .collect::<String>();

        assert!(!string(&controls).chars().any(char::is_control));
    }
}
