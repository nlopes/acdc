//! On-type formatting: list auto-continuation and delimited block auto-close
//!
//! Implements `textDocument/onTypeFormatting` triggered by `\n` (Enter key).
//! When the user presses Enter after a list item, the list marker is
//! automatically inserted on the next line. When Enter is pressed after
//! a delimited block opening delimiter, the matching closing delimiter
//! is inserted below.

use tower_lsp_server::ls_types::{Position, Range, TextEdit};

use super::formatting::{
    collect_protected_ranges, collect_protected_ranges_from_text, is_protected,
};
use crate::state::DocumentState;

/// Compute text edits for on-type formatting (triggered by `\n`).
///
/// Returns `None` if no formatting action applies at the cursor position.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn format_on_type(
    doc: &DocumentState,
    position: Position,
    ch: &str,
) -> Option<Vec<TextEdit>> {
    if ch != "\n" {
        return None;
    }

    let lines: Vec<&str> = doc.text().lines().collect();
    let cursor_line = position.line as usize;

    // The cursor is on the new (empty) line after Enter.
    // The previous line is cursor_line - 1.
    if cursor_line == 0 {
        return None;
    }
    let prev_line_idx = cursor_line - 1;
    let prev_line = lines.get(prev_line_idx)?;

    let protected = if let Some(ast) = doc.ast() {
        collect_protected_ranges(ast.document())
    } else {
        collect_protected_ranges_from_text(doc.text())
    };

    // Don't format inside protected (verbatim) ranges
    if is_protected(prev_line_idx, &protected) {
        return None;
    }

    // Try list continuation first, then block auto-close
    if let Some(edits) = try_list_continuation(prev_line, prev_line_idx, cursor_line) {
        return Some(edits);
    }

    if let Some(edits) = try_block_auto_close(&lines, prev_line, prev_line_idx, cursor_line) {
        return Some(edits);
    }

    None
}

// ── List auto-continuation ────────────────────────────────────────────

/// Attempt list auto-continuation. If the previous line is a list item,
/// insert the marker on the new line (or remove the empty marker).
#[allow(clippy::cast_possible_truncation)]
fn try_list_continuation(
    prev_line: &str,
    prev_line_idx: usize,
    cursor_line: usize,
) -> Option<Vec<TextEdit>> {
    let (marker, content, leading_ws) = detect_list_marker(prev_line)?;

    if content.is_empty() {
        // Empty list item: remove it (user pressed Enter on a blank marker)
        Some(vec![TextEdit {
            range: Range {
                start: Position {
                    line: prev_line_idx as u32,
                    character: 0,
                },
                end: Position {
                    line: cursor_line as u32,
                    character: 0,
                },
            },
            new_text: String::new(),
        }])
    } else {
        // Determine the continuation marker
        let next_marker = next_list_marker(marker);
        let new_text = format!("{leading_ws}{next_marker} ");
        Some(vec![TextEdit {
            range: Range {
                start: Position {
                    line: cursor_line as u32,
                    character: 0,
                },
                end: Position {
                    line: cursor_line as u32,
                    character: 0,
                },
            },
            new_text,
        }])
    }
}

/// Compute the next list marker for continuation.
fn next_list_marker(marker: &str) -> String {
    // Numeric ordered markers: "1." -> "2.", "9." -> "10."
    if marker.len() > 1
        && let Some(num_str) = marker.strip_suffix('.')
        && let Ok(n) = num_str.parse::<u32>()
    {
        return format!("{}.", n + 1);
    }

    // Callout markers: "<1>" -> "<2>", "<.>" stays "<.>"
    if let Some(inner) = marker.strip_prefix('<').and_then(|s| s.strip_suffix('>'))
        && inner != "."
        && let Ok(n) = inner.parse::<u32>()
    {
        return format!("<{}>", n + 1);
    }

    // All other markers repeat as-is (*, **, -, ., .., etc.)
    marker.to_string()
}

/// Detect a list marker on a line.
///
/// Returns `(marker, content, leading_whitespace)` if found.
fn detect_list_marker(line: &str) -> Option<(&str, &str, &str)> {
    let trimmed = line.trim_start();
    let ws_len = line.len() - trimmed.len();
    let leading_ws = &line[..ws_len];

    // Try each pattern in order of specificity
    if let Some((marker, content)) = match_star_marker(trimmed) {
        return Some((marker, content, leading_ws));
    }
    if let Some((marker, content)) = match_dash_marker(trimmed) {
        return Some((marker, content, leading_ws));
    }
    if let Some((marker, content)) = match_dot_marker(trimmed) {
        return Some((marker, content, leading_ws));
    }
    if let Some((marker, content)) = match_numeric_marker(trimmed) {
        return Some((marker, content, leading_ws));
    }
    if let Some((marker, content)) = match_callout_marker(trimmed) {
        return Some((marker, content, leading_ws));
    }

    None
}

/// Match unordered list marker: one or more `*` followed by a space.
fn match_star_marker(trimmed: &str) -> Option<(&str, &str)> {
    let bytes = trimmed.as_bytes();
    if bytes.first() != Some(&b'*') {
        return None;
    }
    let star_end = bytes.iter().position(|&b| b != b'*').unwrap_or(bytes.len());
    // Must be followed by a space (not a bare delimiter like ****)
    if bytes.get(star_end) != Some(&b' ') {
        return None;
    }
    let marker = &trimmed[..star_end];
    let rest = trimmed[star_end + 1..].trim_start();
    Some((marker, rest))
}

/// Match unordered dash list marker: exactly `-` followed by a space.
fn match_dash_marker(trimmed: &str) -> Option<(&str, &str)> {
    if let Some(rest) = trimmed.strip_prefix("- ") {
        Some((&trimmed[..1], rest.trim_start()))
    } else {
        None
    }
}

/// Match ordered dot list marker: one or more `.` followed by a space.
fn match_dot_marker(trimmed: &str) -> Option<(&str, &str)> {
    let bytes = trimmed.as_bytes();
    if bytes.first() != Some(&b'.') {
        return None;
    }
    let dot_end = bytes.iter().position(|&b| b != b'.').unwrap_or(bytes.len());
    // Must be followed by a space (not a bare delimiter like ....)
    if bytes.get(dot_end) != Some(&b' ') {
        return None;
    }
    let marker = &trimmed[..dot_end];
    let rest = trimmed[dot_end + 1..].trim_start();
    Some((marker, rest))
}

/// Match ordered numeric list marker: digits followed by `.` followed by a space.
fn match_numeric_marker(trimmed: &str) -> Option<(&str, &str)> {
    let bytes = trimmed.as_bytes();
    let digit_end = bytes.iter().position(|b| !b.is_ascii_digit()).unwrap_or(0);
    if digit_end == 0 {
        return None;
    }
    if bytes.get(digit_end) != Some(&b'.') {
        return None;
    }
    let marker_end = digit_end + 1;
    if bytes.get(marker_end) != Some(&b' ') {
        return None;
    }
    let marker = &trimmed[..marker_end];
    let rest = trimmed[marker_end + 1..].trim_start();
    Some((marker, rest))
}

/// Match callout list marker: `<digits>` or `<.>` followed by a space.
fn match_callout_marker(trimmed: &str) -> Option<(&str, &str)> {
    if !trimmed.starts_with('<') {
        return None;
    }
    let close = trimmed.find('>')?;
    let inside = &trimmed[1..close];

    // Must be digits or "."
    let valid = inside == "." || inside.chars().all(|c| c.is_ascii_digit());
    if !valid {
        return None;
    }

    let marker_end = close + 1;
    if trimmed.as_bytes().get(marker_end) != Some(&b' ') {
        return None;
    }

    let marker = &trimmed[..marker_end];
    let rest = trimmed[marker_end + 1..].trim_start();
    Some((marker, rest))
}

// ── Block auto-close ──────────────────────────────────────────────────

/// Delimiter specifications: (character, minimum length, exact length or None for >= min).
const BLOCK_DELIMITERS: &[(u8, usize, Option<usize>)] = &[
    (b'-', 4, None),    // listing block: ----
    (b'=', 4, None),    // example block: ====
    (b'.', 4, None),    // literal block: ....
    (b'*', 4, None),    // sidebar block: ****
    (b'_', 4, None),    // quote block: ____
    (b'+', 4, None),    // passthrough block: ++++
    (b'/', 4, None),    // comment block: ////
    (b'-', 2, Some(2)), // open block: -- (exactly 2)
    (b'~', 4, None),    // open block alt: ~~~~
    (b'`', 3, None),    // markdown code: ```
];

/// Check if a line is a block delimiter. Returns the delimiter string if matched.
fn detect_block_delimiter(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let bytes = trimmed.as_bytes();
    let first = *bytes.first()?;

    // For backtick fences, allow a language identifier after the backticks.
    // E.g., "```rust" — the delimiter portion is "```".
    if first == b'`' {
        let backtick_end = bytes.iter().position(|&b| b != b'`').unwrap_or(bytes.len());
        if backtick_end >= 3 {
            return Some(&trimmed[..backtick_end]);
        }
        return None;
    }

    // All other delimiters: all characters must be the same
    if !bytes.iter().all(|&b| b == first) {
        return None;
    }

    let len = trimmed.len();

    for &(delim_char, min_len, exact_len) in BLOCK_DELIMITERS {
        if first == delim_char {
            match exact_len {
                Some(exact) if len == exact => return Some(trimmed),
                None if len >= min_len => return Some(trimmed),
                _ => {}
            }
        }
    }

    None
}

/// Attempt delimited block auto-close.
#[allow(clippy::cast_possible_truncation)]
fn try_block_auto_close(
    lines: &[&str],
    prev_line: &str,
    prev_line_idx: usize,
    cursor_line: usize,
) -> Option<Vec<TextEdit>> {
    let delimiter = detect_block_delimiter(prev_line)?;

    // If this delimiter is closing a previous open one, don't auto-close
    if is_closing_delimiter(lines, delimiter, prev_line_idx) {
        return None;
    }

    // If this delimiter already has a matching close below, don't auto-close
    if has_matching_close(lines, delimiter, prev_line_idx) {
        return None;
    }

    // Insert: newline + closing delimiter + newline
    let new_text = format!("\n{delimiter}\n");
    Some(vec![TextEdit {
        range: Range {
            start: Position {
                line: cursor_line as u32,
                character: 0,
            },
            end: Position {
                line: cursor_line as u32,
                character: 0,
            },
        },
        new_text,
    }])
}

/// Check if the delimiter at `line_idx` is closing a previously opened block.
///
/// Counts matching delimiters above; odd count means this is a closer.
fn is_closing_delimiter(lines: &[&str], delimiter: &str, line_idx: usize) -> bool {
    let mut count = 0;
    for line in lines.iter().take(line_idx) {
        if let Some(d) = detect_block_delimiter(line)
            && d == delimiter
        {
            count += 1;
        }
    }
    // Odd count means there's an unmatched opener above
    count % 2 == 1
}

/// Check if an opening delimiter has a matching close below it.
fn has_matching_close(lines: &[&str], delimiter: &str, open_line_idx: usize) -> bool {
    for line in lines.iter().skip(open_line_idx + 1) {
        if let Some(d) = detect_block_delimiter(line)
            && d == delimiter
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn format_on_enter(text: &str, line: u32) -> Option<Vec<TextEdit>> {
        let doc = DocumentState::new_failure(text.to_string(), 0, vec![]);
        format_on_type(&doc, Position { line, character: 0 }, "\n")
    }

    // ── List continuation: unordered ──────────────────────────────────

    #[test]
    fn unordered_star_continuation() {
        let edits = format_on_enter("* item\n", 1).unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "* ");
    }

    #[test]
    fn unordered_star_nested_continuation() {
        let edits = format_on_enter("** nested item\n", 1).unwrap();
        assert_eq!(edits[0].new_text, "** ");
    }

    #[test]
    fn unordered_dash_continuation() {
        let edits = format_on_enter("- item\n", 1).unwrap();
        assert_eq!(edits[0].new_text, "- ");
    }

    #[test]
    fn empty_unordered_removes_marker() {
        let edits = format_on_enter("* \n", 1).unwrap();
        assert_eq!(edits[0].new_text, ""); // deletion
        assert_eq!(edits[0].range.start.line, 0);
        assert_eq!(edits[0].range.end.line, 1);
    }

    // ── List continuation: ordered ────────────────────────────────────

    #[test]
    fn ordered_dot_continuation() {
        let edits = format_on_enter(". first\n", 1).unwrap();
        assert_eq!(edits[0].new_text, ". ");
    }

    #[test]
    fn ordered_nested_dot_continuation() {
        let edits = format_on_enter(".. nested\n", 1).unwrap();
        assert_eq!(edits[0].new_text, ".. ");
    }

    #[test]
    fn ordered_numeric_continuation() {
        let edits = format_on_enter("1. first\n", 1).unwrap();
        assert_eq!(edits[0].new_text, "2. ");
    }

    #[test]
    fn ordered_numeric_increment() {
        let edits = format_on_enter("9. ninth\n", 1).unwrap();
        assert_eq!(edits[0].new_text, "10. ");
    }

    // ── List continuation: callout ────────────────────────────────────

    #[test]
    fn callout_numeric_continuation() {
        let edits = format_on_enter("<1> first callout\n", 1).unwrap();
        assert_eq!(edits[0].new_text, "<2> ");
    }

    #[test]
    fn callout_auto_continuation() {
        let edits = format_on_enter("<.> auto callout\n", 1).unwrap();
        assert_eq!(edits[0].new_text, "<.> ");
    }

    // ── List continuation: whitespace ─────────────────────────────────

    #[test]
    fn preserves_leading_whitespace() {
        let edits = format_on_enter("  * indented\n", 1).unwrap();
        assert_eq!(edits[0].new_text, "  * ");
    }

    // ── No continuation on non-list lines ─────────────────────────────

    #[test]
    fn no_continuation_on_plain_text() {
        assert!(format_on_enter("Just a paragraph.\n", 1).is_none());
    }

    #[test]
    fn no_continuation_on_heading() {
        assert!(format_on_enter("== Heading\n", 1).is_none());
    }

    // ── Block auto-close ──────────────────────────────────────────────

    #[test]
    fn auto_close_listing_block() {
        let edits = format_on_enter("----\n", 1).unwrap();
        assert_eq!(edits[0].new_text, "\n----\n");
    }

    #[test]
    fn auto_close_example_block() {
        let edits = format_on_enter("====\n", 1).unwrap();
        assert_eq!(edits[0].new_text, "\n====\n");
    }

    #[test]
    fn auto_close_literal_block() {
        let edits = format_on_enter("....\n", 1).unwrap();
        assert_eq!(edits[0].new_text, "\n....\n");
    }

    #[test]
    fn auto_close_sidebar_block() {
        let edits = format_on_enter("****\n", 1).unwrap();
        assert_eq!(edits[0].new_text, "\n****\n");
    }

    #[test]
    fn auto_close_quote_block() {
        let edits = format_on_enter("____\n", 1).unwrap();
        assert_eq!(edits[0].new_text, "\n____\n");
    }

    #[test]
    fn auto_close_passthrough_block() {
        let edits = format_on_enter("++++\n", 1).unwrap();
        assert_eq!(edits[0].new_text, "\n++++\n");
    }

    #[test]
    fn auto_close_comment_block() {
        let edits = format_on_enter("////\n", 1).unwrap();
        assert_eq!(edits[0].new_text, "\n////\n");
    }

    #[test]
    fn auto_close_open_block() {
        let edits = format_on_enter("--\n", 1).unwrap();
        assert_eq!(edits[0].new_text, "\n--\n");
    }

    #[test]
    fn auto_close_tilde_block() {
        let edits = format_on_enter("~~~~\n", 1).unwrap();
        assert_eq!(edits[0].new_text, "\n~~~~\n");
    }

    #[test]
    fn auto_close_markdown_code() {
        let edits = format_on_enter("```\n", 1).unwrap();
        assert_eq!(edits[0].new_text, "\n```\n");
    }

    #[test]
    fn auto_close_markdown_code_with_language() {
        let edits = format_on_enter("```rust\n", 1).unwrap();
        assert_eq!(edits[0].new_text, "\n```\n");
    }

    #[test]
    fn auto_close_longer_delimiter() {
        let edits = format_on_enter("------\n", 1).unwrap();
        assert_eq!(edits[0].new_text, "\n------\n");
    }

    #[test]
    fn no_auto_close_when_already_paired() {
        // The delimiter on line 0 already has a matching close on line 2
        assert!(format_on_enter("----\ncontent\n----\n", 1).is_none());
    }

    #[test]
    fn no_auto_close_for_closing_delimiter() {
        // Line 2 is the closing delimiter — pressing Enter after it shouldn't auto-close
        assert!(format_on_enter("----\ncontent\n----\n", 3).is_none());
    }

    // ── Protected range ───────────────────────────────────────────────

    #[test]
    fn no_continuation_in_listing_block() {
        // Inside a listing block, "* item" is code, not a list
        assert!(format_on_enter("----\n* item\n----\n", 2).is_none());
    }

    // ── Trigger character filtering ───────────────────────────────────

    #[test]
    fn ignores_non_newline_trigger() {
        let doc = DocumentState::new_failure("* item\n".to_string(), 0, vec![]);
        assert!(
            format_on_type(
                &doc,
                Position {
                    line: 1,
                    character: 0
                },
                "a"
            )
            .is_none()
        );
    }

    // ── Edge cases ────────────────────────────────────────────────────

    #[test]
    fn cursor_at_line_zero_returns_none() {
        assert!(format_on_enter("text\n", 0).is_none());
    }

    #[test]
    fn three_dashes_not_a_delimiter() {
        // --- is not a valid AsciiDoc delimiter
        assert!(format_on_enter("---\n", 1).is_none());
    }

    #[test]
    fn single_backtick_not_a_delimiter() {
        assert!(format_on_enter("`\n", 1).is_none());
    }

    #[test]
    fn two_backticks_not_a_delimiter() {
        assert!(format_on_enter("``\n", 1).is_none());
    }

    // ── Unit tests for internal helpers ───────────────────────────────

    #[test]
    fn next_marker_numeric() {
        assert_eq!(next_list_marker("1."), "2.");
        assert_eq!(next_list_marker("9."), "10.");
        assert_eq!(next_list_marker("99."), "100.");
    }

    #[test]
    fn next_marker_callout() {
        assert_eq!(next_list_marker("<1>"), "<2>");
        assert_eq!(next_list_marker("<.>"), "<.>");
    }

    #[test]
    fn next_marker_preserves_others() {
        assert_eq!(next_list_marker("*"), "*");
        assert_eq!(next_list_marker("**"), "**");
        assert_eq!(next_list_marker("-"), "-");
        assert_eq!(next_list_marker("."), ".");
        assert_eq!(next_list_marker(".."), "..");
    }

    #[test]
    fn detect_delimiter_variations() {
        assert_eq!(detect_block_delimiter("----"), Some("----"));
        assert_eq!(detect_block_delimiter("------"), Some("------"));
        assert_eq!(detect_block_delimiter("--"), Some("--"));
        assert_eq!(detect_block_delimiter("---"), None);
        assert_eq!(detect_block_delimiter("```"), Some("```"));
        assert_eq!(detect_block_delimiter("```rust"), Some("```"));
        assert_eq!(detect_block_delimiter("````"), Some("````"));
        assert_eq!(detect_block_delimiter("``"), None);
        assert_eq!(detect_block_delimiter("===="), Some("===="));
        assert_eq!(detect_block_delimiter("...."), Some("...."));
        assert_eq!(detect_block_delimiter("****"), Some("****"));
        assert_eq!(detect_block_delimiter(""), None);
    }
}
