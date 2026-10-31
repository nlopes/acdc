//! Document formatting: normalize whitespace for clean diffs
//!
//! Implements `textDocument/formatting` and `textDocument/rangeFormatting`.
//! Uses a text-based approach with AST guidance: operates on source text
//! line-by-line, using the AST only to identify verbatim blocks where
//! formatting must not be applied.

use std::ops;

use acdc_parser::ast::{BlockKind, BlockRef, DocumentRef, StructuralRef};
use tower_lsp_server::ls_types::{FormattingOptions, Position, Range, TextEdit};

use crate::{
    convert::{
        LineIndex, byte_range_to_utf16_range, is_primary_location, normalize_range, to_lsp_u32,
    },
    state::DocumentState,
};

/// A line range where formatting must not be applied (e.g., inside listing blocks).
/// Lines are 0-indexed.
#[derive(Debug, Clone)]
pub(crate) struct ProtectedRange {
    start_line: usize,
    end_line: usize,
}

/// Format an entire document, returning a list of text edits.
#[must_use]
pub(crate) fn format_document(doc: &DocumentState, options: &FormattingOptions) -> Vec<TextEdit> {
    let lines: Vec<&str> = doc.text().lines().collect();
    let line_count = lines.len();

    let protected = if let Some(ast) = doc.ast() {
        collect_protected_ranges(ast.document_ref(), doc.text())
    } else {
        collect_protected_ranges_from_text(doc.text())
    };

    let range = 0..line_count;
    let mut edits = Vec::new();

    edits.extend(trim_trailing_whitespace(&lines, &protected, range.clone()));
    edits.extend(collapse_blank_lines(
        &lines,
        &protected,
        range.clone(),
        doc.text(),
    ));

    if let Some(ast) = doc.ast() {
        edits.extend(ensure_block_separation(
            &lines,
            ast.document_ref(),
            doc.text(),
            &protected,
            range,
        ));
    }

    edits.extend(normalize_final_newline(doc.text(), options, &protected));

    coalesce_deletions(edits)
}

/// Format a range of a document, returning a list of text edits.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn format_range(
    doc: &DocumentState,
    range: &Range,
    options: &FormattingOptions,
) -> Vec<TextEdit> {
    let Some(range) = normalize_range(doc.text(), *range) else {
        return Vec::new();
    };
    let lines: Vec<&str> = doc.text().lines().collect();
    let line_count = lines.len();

    // Expand to full lines
    let start_line = range.start.line as usize;
    let last_selected = if range.end.character == 0 && range.end.line > range.start.line {
        range.end.line - 1
    } else {
        range.end.line
    };
    let end_line = (last_selected as usize).min(line_count.saturating_sub(1));
    let line_range = start_line..end_line + 1;

    let protected = if let Some(ast) = doc.ast() {
        collect_protected_ranges(ast.document_ref(), doc.text())
    } else {
        collect_protected_ranges_from_text(doc.text())
    };

    let mut edits = Vec::new();

    edits.extend(trim_trailing_whitespace(
        &lines,
        &protected,
        line_range.clone(),
    ));
    edits.extend(collapse_blank_lines(
        &lines,
        &protected,
        line_range.clone(),
        doc.text(),
    ));

    if let Some(ast) = doc.ast() {
        edits.extend(ensure_block_separation(
            &lines,
            ast.document_ref(),
            doc.text(),
            &protected,
            line_range.clone(),
        ));
    }

    // Only normalize final newline if range includes the last line
    if end_line >= line_count.saturating_sub(1) {
        edits.extend(normalize_final_newline(doc.text(), options, &protected));
    }

    coalesce_deletions(edits)
}

/// Protect verbatim blocks in the original text and primary-source AST ranges.
pub(crate) fn collect_protected_ranges(ast: DocumentRef<'_>, text: &str) -> Vec<ProtectedRange> {
    let mut ranges = collect_protected_ranges_from_text(text);
    for block in ast.blocks() {
        collect_protected_ranges_from_block(block, &mut ranges);
    }
    ranges
}

fn collect_protected_ranges_from_block(block: BlockRef<'_>, ranges: &mut Vec<ProtectedRange>) {
    match block.kind() {
        BlockKind::Listing(_)
        | BlockKind::Literal(_)
        | BlockKind::Passthrough(_)
        | BlockKind::DelimitedComment(_)
        | BlockKind::Verse(_)
        | BlockKind::Stem(_) => {
            let location = block.location();
            if is_primary_location(location) {
                ranges.push(ProtectedRange {
                    start_line: location.start().line().saturating_sub(1) as usize,
                    end_line: location.end().line().saturating_sub(1) as usize,
                });
            }
            return;
        }
        BlockKind::Section(_)
        | BlockKind::Admonition(_)
        | BlockKind::Example(_)
        | BlockKind::Open(_)
        | BlockKind::Sidebar(_)
        | BlockKind::Quote(_) => {}
        BlockKind::TableOfContents(_)
        | BlockKind::DiscreteHeader(_)
        | BlockKind::DocumentAttribute(_)
        | BlockKind::ThematicBreak(_)
        | BlockKind::PageBreak(_)
        | BlockKind::UnorderedList(_)
        | BlockKind::OrderedList(_)
        | BlockKind::CalloutList(_)
        | BlockKind::DescriptionList(_)
        | BlockKind::Paragraph(_)
        | BlockKind::Image(_)
        | BlockKind::Audio(_)
        | BlockKind::Video(_)
        | BlockKind::Comment(_)
        | BlockKind::Table(_)
        | _ => return,
    }
    for child in StructuralRef::from(block).children() {
        if let StructuralRef::Block(block) = child {
            collect_protected_ranges_from_block(block, ranges);
        }
    }
}

/// Protect verbatim source through its closing delimiter or end of file.
pub(crate) fn collect_protected_ranges_from_text(text: &str) -> Vec<ProtectedRange> {
    let mut ranges = Vec::new();
    let delimiter_chars = ['-', '.', '+', '/'];

    let mut open_delimiter: Option<(&str, usize)> = None;

    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if let Some((open_delim, start)) = &open_delimiter {
            if trimmed == *open_delim {
                ranges.push(ProtectedRange {
                    start_line: *start,
                    end_line: line_idx,
                });
                open_delimiter = None;
            }
        } else if trimmed.len() >= 4 {
            // Check if line is a delimiter (4+ of the same delimiter char)
            if let Some(&first) = trimmed.as_bytes().first() {
                let ch = char::from(first);
                if delimiter_chars.contains(&ch) && trimmed.bytes().all(|b| b == first) {
                    open_delimiter = Some((trimmed, line_idx));
                }
            }
        }
    }

    if let Some((_, start_line)) = open_delimiter {
        ranges.push(ProtectedRange {
            start_line,
            end_line: text.lines().count().saturating_sub(1),
        });
    }
    ranges
}

/// Check if a line falls within any protected range.
pub(crate) fn is_protected(line: usize, ranges: &[ProtectedRange]) -> bool {
    ranges
        .iter()
        .any(|r| line >= r.start_line && line <= r.end_line)
}

pub(super) fn is_protected_after_opener(line: usize, ranges: &[ProtectedRange]) -> bool {
    ranges
        .iter()
        .any(|range| range.start_line < line && line <= range.end_line)
}

/// Generate edits to trim trailing whitespace from lines.
#[allow(clippy::cast_possible_truncation)]
fn trim_trailing_whitespace(
    lines: &[&str],
    protected: &[ProtectedRange],
    range: ops::Range<usize>,
) -> Vec<TextEdit> {
    let mut edits = Vec::new();

    for line_idx in range {
        let Some(line) = lines.get(line_idx) else {
            continue;
        };
        if is_protected(line_idx, protected) {
            continue;
        }

        let trimmed = line.trim_end();
        if trimmed.len() < line.len() {
            let start_char = to_lsp_u32(trimmed.encode_utf16().count());
            let end_char = to_lsp_u32(line.encode_utf16().count());
            edits.push(TextEdit {
                range: Range {
                    start: Position {
                        line: line_idx as u32,
                        character: start_char,
                    },
                    end: Position {
                        line: line_idx as u32,
                        character: end_char,
                    },
                },
                new_text: String::new(),
            });
        }
    }

    edits
}

/// Generate edits to collapse multiple consecutive blank lines into one.
#[allow(clippy::cast_possible_truncation)]
fn collapse_blank_lines(
    lines: &[&str],
    protected: &[ProtectedRange],
    range: ops::Range<usize>,
    text: &str,
) -> Vec<TextEdit> {
    let mut edits = Vec::new();
    let mut blank_run_start: Option<usize> = None;

    for line_idx in range.clone() {
        let Some(line) = lines.get(line_idx) else {
            continue;
        };

        let is_blank = line.trim().is_empty();
        let is_prot = is_protected(line_idx, protected);

        if is_blank && !is_prot {
            if blank_run_start.is_none() {
                blank_run_start = Some(line_idx);
            }
        } else {
            if let Some(start) = blank_run_start {
                let run_len = line_idx - start;
                if run_len > 1 {
                    // Keep the first blank line, remove the rest
                    edits.push(TextEdit {
                        range: Range {
                            start: Position {
                                line: (start + 1) as u32,
                                character: 0,
                            },
                            end: Position {
                                line: line_idx as u32,
                                character: 0,
                            },
                        },
                        new_text: String::new(),
                    });
                }
            }
            blank_run_start = None;
        }
    }

    // Handle trailing blank run at end of range
    if let Some(start) = blank_run_start {
        let end = range.end.min(lines.len());
        let run_len = end - start;
        if run_len > 1 {
            edits.push(TextEdit {
                range: Range {
                    start: Position {
                        line: (start + 1) as u32,
                        character: 0,
                    },
                    end: if end < lines.len() || text.ends_with('\n') {
                        Position::new(to_lsp_u32(end), 0)
                    } else {
                        Position::new(
                            to_lsp_u32(end - 1),
                            to_lsp_u32(lines.last().map_or(0, |line| line.encode_utf16().count())),
                        )
                    },
                },
                new_text: String::new(),
            });
        }
    }

    edits
}

/// Generate edits to ensure a blank line between consecutive top-level blocks.
#[allow(clippy::cast_possible_truncation)]
fn ensure_block_separation(
    lines: &[&str],
    ast: DocumentRef<'_>,
    text: &str,
    protected: &[ProtectedRange],
    range: ops::Range<usize>,
) -> Vec<TextEdit> {
    let mut edits = Vec::new();
    let source = LineIndex::new(text);

    for (prev_block, curr_block) in ast.blocks().zip(ast.blocks().skip(1)) {
        let previous = prev_block.location();
        let current = curr_block.location();
        if !is_primary_location(previous)
            || !is_primary_location(current)
            || source.range(&previous.into()).is_none()
            || source.range(&current.into()).is_none()
        {
            continue;
        }

        let prev_end_line = previous.end().line().saturating_sub(1) as usize;
        let curr_start_line = current.start().line().saturating_sub(1) as usize;

        // An include can close a parsed block before its closing delimiter in
        // the editor text. Do not add separators inside that original block.
        if protected
            .iter()
            .any(|span| span.start_line <= prev_end_line && prev_end_line < span.end_line)
        {
            continue;
        }

        // Only process blocks within our range
        if prev_end_line < range.start || curr_start_line >= range.end {
            continue;
        }

        // If blocks are immediately adjacent (no blank line between them)
        if curr_start_line == prev_end_line + 1 {
            // Don't insert between consecutive document attributes
            if matches!(prev_block.kind(), BlockKind::DocumentAttribute(_))
                && matches!(curr_block.kind(), BlockKind::DocumentAttribute(_))
            {
                continue;
            }

            // Insert a blank line at the end of the previous block's last line
            if let Some(line) = lines.get(prev_end_line) {
                let col = to_lsp_u32(line.encode_utf16().count());
                edits.push(TextEdit {
                    range: Range {
                        start: Position {
                            line: prev_end_line as u32,
                            character: col,
                        },
                        end: Position {
                            line: prev_end_line as u32,
                            character: col,
                        },
                    },
                    new_text: line_ending(text).to_string(),
                });
            }
        }
    }

    edits
}

pub(super) fn line_ending(text: &str) -> &str {
    if text
        .split_once('\n')
        .is_some_and(|(prefix, _)| prefix.ends_with('\r'))
    {
        "\r\n"
    } else {
        "\n"
    }
}

fn normalize_final_newline(
    text: &str,
    options: &FormattingOptions,
    protected: &[ProtectedRange],
) -> Vec<TextEdit> {
    let insert_final = options.insert_final_newline.unwrap_or(true);
    let trim_final = options.trim_final_newlines.unwrap_or(true);
    let last_line = text.lines().count().saturating_sub(1);
    let content = if is_protected(last_line, protected) {
        text.trim_end_matches(['\r', '\n'])
    } else {
        text.trim_end()
    };
    let trailing = text.get(content.len()..).unwrap_or_default();
    let bytes = if let Some(newline) = trailing.find('\n') {
        // Keep the first line ending, including its CR when present.
        let keep_end = content.len() + newline + 1;
        if !trim_final || keep_end == text.len() {
            return Vec::new();
        }
        keep_end..text.len()
    } else if insert_final {
        text.len()..text.len()
    } else {
        return Vec::new();
    };
    let Some(range) = byte_range_to_utf16_range(text, bytes.clone()) else {
        return Vec::new();
    };
    vec![TextEdit {
        range,
        new_text: if bytes.is_empty() {
            line_ending(text).to_string()
        } else {
            String::new()
        },
    }]
}

fn coalesce_deletions(mut edits: Vec<TextEdit>) -> Vec<TextEdit> {
    edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
    let mut result: Vec<TextEdit> = Vec::with_capacity(edits.len());
    for edit in edits {
        if let Some(previous) = result.last_mut()
            && previous.new_text.is_empty()
            && edit.new_text.is_empty()
            && edit.range.start <= previous.range.end
        {
            previous.range.end = previous.range.end.max(edit.range.end);
        } else {
            result.push(edit);
        }
    }
    result
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use tower_lsp_server::ls_types::Uri;

    use super::*;
    use crate::{convert::position_to_offset, state::Workspace};

    fn make_options() -> FormattingOptions {
        FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            trim_trailing_whitespace: Some(true),
            insert_final_newline: Some(true),
            trim_final_newlines: Some(true),
            ..Default::default()
        }
    }

    /// Helper: format a document and apply edits to verify the result.
    fn format_and_apply(src: &str) -> String {
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>().unwrap();
        workspace.update_document(uri.clone(), src.to_string(), 1);
        let doc = workspace.get_document(&uri).unwrap();
        let options = make_options();
        let edits = format_document(&doc, &options);
        apply_edits(src, &edits)
    }

    /// Apply text edits to source text, producing the formatted result.
    #[allow(clippy::cast_possible_truncation)]
    fn apply_edits(src: &str, edits: &[TextEdit]) -> String {
        if edits.is_empty() {
            return src.to_string();
        }

        // Sort edits by position (reverse order for safe application)
        let mut sorted: Vec<&TextEdit> = edits.iter().collect();
        sorted.sort_by(|a, b| {
            b.range
                .start
                .line
                .cmp(&a.range.start.line)
                .then(b.range.start.character.cmp(&a.range.start.character))
        });

        let mut result = src.to_string();
        for edit in sorted {
            let start_offset = position_to_offset(&result, edit.range.start).unwrap();
            let end_offset = position_to_offset(&result, edit.range.end).unwrap();
            result.replace_range(start_offset..end_offset, &edit.new_text);
        }

        result
    }

    // ── Trailing whitespace tests ───────────────────────────────────────

    #[test]
    fn test_trim_trailing_whitespace() {
        let src = "= Title  \n\nParagraph with spaces   \n";
        let result = format_and_apply(src);
        assert_eq!(result, "= Title\n\nParagraph with spaces\n");
    }

    #[test]
    fn test_trim_trailing_tabs() {
        let src = "= Title\t\t\n\nText\t\n";
        let result = format_and_apply(src);
        assert_eq!(result, "= Title\n\nText\n");
    }

    #[test]
    fn test_no_trim_in_listing_block() {
        let src = "= Title\n\n----\ncode with spaces   \n----\n";
        let result = format_and_apply(src);
        assert_eq!(result, "= Title\n\n----\ncode with spaces   \n----\n");
    }

    #[test]
    fn test_no_trim_in_literal_block() {
        let src = "= Title\n\n....\ntext with spaces   \n....\n";
        let result = format_and_apply(src);
        assert_eq!(result, "= Title\n\n....\ntext with spaces   \n....\n");
    }

    // ── Blank line collapse tests ───────────────────────────────────────

    #[test]
    fn test_collapse_multiple_blank_lines() {
        let src = "= Title\n\n\n\nParagraph\n";
        let result = format_and_apply(src);
        assert_eq!(result, "= Title\n\nParagraph\n");
    }

    #[test]
    fn test_collapse_two_blank_lines() {
        let src = "= Title\n\n\nParagraph\n";
        let result = format_and_apply(src);
        assert_eq!(result, "= Title\n\nParagraph\n");
    }

    #[test]
    fn test_single_blank_line_unchanged() {
        let src = "= Title\n\nParagraph\n";
        let result = format_and_apply(src);
        assert_eq!(result, "= Title\n\nParagraph\n");
    }

    #[test]
    fn test_no_collapse_in_verbatim() {
        let src = "= Title\n\n----\n\n\n\ncode\n----\n";
        let result = format_and_apply(src);
        assert_eq!(result, "= Title\n\n----\n\n\n\ncode\n----\n");
    }

    // ── Final newline tests ─────────────────────────────────────────────

    #[test]
    fn test_final_newline_added() {
        let src = "= Title\n\nParagraph";
        let result = format_and_apply(src);
        assert_eq!(result, "= Title\n\nParagraph\n");
    }

    #[test]
    fn test_final_newline_trimmed() {
        let src = "= Title\n\nParagraph\n\n\n\n";
        let result = format_and_apply(src);
        assert_eq!(result, "= Title\n\nParagraph\n");
    }

    #[test]
    fn test_final_newline_respects_option() {
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>().unwrap();
        let src = "= Title\n\nParagraph";
        workspace.update_document(uri.clone(), src.to_string(), 1);
        let doc = workspace.get_document(&uri).unwrap();

        let mut options = make_options();
        options.insert_final_newline = Some(false);
        let edits = format_document(&doc, &options);
        let result = apply_edits(src, &edits);
        assert_eq!(result, "= Title\n\nParagraph");
    }

    // ── Block separation tests ──────────────────────────────────────────

    #[test]
    fn test_block_separation_inserted() {
        // Two sections at same level with no blank line between them
        let src = "== Section 1\n== Section 2\n";
        let result = format_and_apply(src);
        assert!(
            result.contains("== Section 1\n\n== Section 2"),
            "Expected blank line between sections, got: {result}"
        );
    }

    #[test]
    fn test_no_separation_between_consecutive_attributes() {
        let src = "= Title\n:attr1: value1\n:attr2: value2\n\nText\n";
        let result = format_and_apply(src);
        // Consecutive attributes should NOT have blank lines inserted
        assert!(
            result.contains(":attr1: value1\n:attr2: value2"),
            "Consecutive attributes should not be separated, got: {result}"
        );
    }

    // ── Integration tests ───────────────────────────────────────────────

    #[test]
    fn test_format_full_document() {
        let src = "= Title  \n\n\n\nParagraph with trailing   \n\n\n\n----\ncode   \n----\n\nAnother paragraph  \n";
        let result = format_and_apply(src);
        assert_eq!(
            result,
            "= Title\n\nParagraph with trailing\n\n----\ncode   \n----\n\nAnother paragraph\n"
        );
    }

    #[test]
    fn test_format_empty_document() {
        let src = "";
        let result = format_and_apply(src);
        assert_eq!(result, "\n");
    }

    #[test]
    fn test_idempotent() {
        let src = "= Title\n\nParagraph text.\n\n----\ncode block\n----\n\nAnother paragraph.\n";
        let result = format_and_apply(src);
        assert_eq!(
            result, src,
            "Formatting an already-formatted document should produce no changes"
        );

        // Also verify no edits are produced
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>().unwrap();
        workspace.update_document(uri.clone(), src.to_string(), 1);
        let doc = workspace.get_document(&uri).unwrap();
        let edits = format_document(&doc, &make_options());
        assert!(
            edits.is_empty(),
            "Expected no edits for already-formatted document, got {edits:?}"
        );
    }

    #[test]
    fn test_range_formatting_only_affects_range() {
        let src = "= Title  \n\nParagraph  \n\nAnother  \n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>().unwrap();
        workspace.update_document(uri.clone(), src.to_string(), 1);
        let doc = workspace.get_document(&uri).unwrap();

        // Only format lines 2-2 (the "Paragraph  " line)
        let range = Range {
            start: Position {
                line: 2,
                character: 0,
            },
            end: Position {
                line: 2,
                character: 11,
            },
        };
        let options = make_options();
        let edits = format_range(&doc, &range, &options);
        let result = apply_edits(src, &edits);

        // "Paragraph  " should be trimmed, but "Title  " and "Another  " untouched
        assert!(
            result.contains("= Title  "),
            "Title should not be changed: {result}"
        );
        assert!(
            result.contains("\nParagraph\n"),
            "Paragraph should be trimmed: {result}"
        );
        assert!(
            result.contains("Another  "),
            "Another should not be changed: {result}"
        );
    }

    #[test]
    fn test_parse_failed_document_still_formats() {
        let src = "Some text  \n\n\n\nMore text  \n";
        let doc = DocumentState::new_failure(src.to_string(), 1);

        let options = make_options();
        let edits = format_document(&doc, &options);
        let result = apply_edits(src, &edits);

        assert_eq!(result, "Some text\n\nMore text\n");
    }

    // ── Protected range fallback tests ──────────────────────────────────

    #[test]
    fn test_text_based_protected_ranges() {
        let text = "Line 1\n----\ncode\n----\nLine 5\n";
        let ranges = collect_protected_ranges_from_text(text);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start_line, 1);
        assert_eq!(ranges[0].end_line, 3);
    }

    #[test]
    fn test_text_based_multiple_protected_ranges() {
        let text = "Text\n----\ncode\n----\nGap\n....\nliteral\n....\n";
        let ranges = collect_protected_ranges_from_text(text);
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0].start_line, 1);
        assert_eq!(ranges[0].end_line, 3);
        assert_eq!(ranges[1].start_line, 5);
        assert_eq!(ranges[1].end_line, 7);
    }
}
