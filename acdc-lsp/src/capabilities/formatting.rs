//! Document formatting: normalize whitespace for clean diffs
//!
//! Implements `textDocument/formatting` and `textDocument/rangeFormatting`.
//! Uses a text-based approach with AST guidance: operates on source text
//! line-by-line, using the AST only to identify verbatim blocks where
//! formatting must not be applied.

use std::ops;

use acdc_parser::{Block, DelimitedBlockType, Document, Location};
use tower_lsp::lsp_types::{FormattingOptions, Position, Range, TextEdit};

use crate::state::DocumentState;

/// A line range where formatting must not be applied (e.g., inside listing blocks).
/// Lines are 0-indexed.
#[derive(Debug, Clone)]
struct ProtectedRange {
    start_line: usize,
    end_line: usize,
}

/// Format an entire document, returning a list of text edits.
#[must_use]
pub fn format_document(doc: &DocumentState, options: &FormattingOptions) -> Vec<TextEdit> {
    let lines: Vec<&str> = doc.text.lines().collect();
    let line_count = lines.len();

    let protected = if let Some(ast) = &doc.ast {
        collect_protected_ranges(ast)
    } else {
        collect_protected_ranges_from_text(&doc.text)
    };

    let range = 0..line_count;
    let mut edits = Vec::new();

    edits.extend(trim_trailing_whitespace(&lines, &protected, range.clone()));
    edits.extend(collapse_blank_lines(&lines, &protected, range.clone()));

    if let Some(ast) = &doc.ast {
        edits.extend(ensure_block_separation(&lines, ast, range));
    }

    edits.extend(normalize_final_newline(&doc.text, options));

    edits
}

/// Format a range of a document, returning a list of text edits.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn format_range(
    doc: &DocumentState,
    range: &Range,
    options: &FormattingOptions,
) -> Vec<TextEdit> {
    let lines: Vec<&str> = doc.text.lines().collect();
    let line_count = lines.len();

    // Expand to full lines
    let start_line = range.start.line as usize;
    let end_line = (range.end.line as usize).min(line_count.saturating_sub(1));
    let line_range = start_line..end_line + 1;

    let protected = if let Some(ast) = &doc.ast {
        collect_protected_ranges(ast)
    } else {
        collect_protected_ranges_from_text(&doc.text)
    };

    let mut edits = Vec::new();

    edits.extend(trim_trailing_whitespace(
        &lines,
        &protected,
        line_range.clone(),
    ));
    edits.extend(collapse_blank_lines(&lines, &protected, line_range.clone()));

    if let Some(ast) = &doc.ast {
        edits.extend(ensure_block_separation(&lines, ast, line_range.clone()));
    }

    // Only normalize final newline if range includes the last line
    if end_line >= line_count.saturating_sub(1) {
        edits.extend(normalize_final_newline(&doc.text, options));
    }

    edits
}

/// Collect protected line ranges from the AST by finding verbatim delimited blocks.
fn collect_protected_ranges(ast: &Document) -> Vec<ProtectedRange> {
    let mut ranges = Vec::new();
    collect_protected_ranges_from_blocks(&ast.blocks, &mut ranges);
    ranges
}

/// Recursively walk blocks to find verbatim delimited blocks.
fn collect_protected_ranges_from_blocks(blocks: &[Block], ranges: &mut Vec<ProtectedRange>) {
    for block in blocks {
        match block {
            Block::DelimitedBlock(db) => {
                if is_verbatim_block_type(&db.inner) {
                    // Location is 1-indexed, convert to 0-indexed
                    ranges.push(ProtectedRange {
                        start_line: db.location.start.line.saturating_sub(1),
                        end_line: db.location.end.line.saturating_sub(1),
                    });
                } else {
                    // Non-verbatim delimited blocks can contain nested verbatim blocks
                    match &db.inner {
                        DelimitedBlockType::DelimitedExample(nested)
                        | DelimitedBlockType::DelimitedOpen(nested)
                        | DelimitedBlockType::DelimitedSidebar(nested)
                        | DelimitedBlockType::DelimitedQuote(nested) => {
                            collect_protected_ranges_from_blocks(nested, ranges);
                        }
                        DelimitedBlockType::DelimitedComment(_)
                        | DelimitedBlockType::DelimitedListing(_)
                        | DelimitedBlockType::DelimitedLiteral(_)
                        | DelimitedBlockType::DelimitedTable(_)
                        | DelimitedBlockType::DelimitedPass(_)
                        | DelimitedBlockType::DelimitedVerse(_)
                        | DelimitedBlockType::DelimitedStem(_)
                        // non_exhaustive
                        | _ => {}
                    }
                }
            }
            Block::Section(s) => {
                collect_protected_ranges_from_blocks(&s.content, ranges);
            }
            Block::Admonition(a) => {
                collect_protected_ranges_from_blocks(&a.blocks, ranges);
            }
            Block::TableOfContents(_)
            | Block::DiscreteHeader(_)
            | Block::DocumentAttribute(_)
            | Block::ThematicBreak(_)
            | Block::PageBreak(_)
            | Block::UnorderedList(_)
            | Block::OrderedList(_)
            | Block::CalloutList(_)
            | Block::DescriptionList(_)
            | Block::Paragraph(_)
            | Block::Image(_)
            | Block::Audio(_)
            | Block::Video(_)
            | Block::Comment(_)
            // non_exhaustive
            | _ => {}
        }
    }
}

/// Check if a delimited block type is verbatim (content should not be formatted).
fn is_verbatim_block_type(inner: &DelimitedBlockType) -> bool {
    matches!(
        inner,
        DelimitedBlockType::DelimitedListing(_)
            | DelimitedBlockType::DelimitedLiteral(_)
            | DelimitedBlockType::DelimitedPass(_)
            | DelimitedBlockType::DelimitedComment(_)
            | DelimitedBlockType::DelimitedVerse(_)
            | DelimitedBlockType::DelimitedStem(_)
    )
}

/// Fallback: detect verbatim block ranges from raw text when AST is unavailable.
fn collect_protected_ranges_from_text(text: &str) -> Vec<ProtectedRange> {
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

    ranges
}

/// Check if a line falls within any protected range.
fn is_protected(line: usize, ranges: &[ProtectedRange]) -> bool {
    ranges
        .iter()
        .any(|r| line >= r.start_line && line <= r.end_line)
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
            let start_char = trimmed.len() as u32;
            let end_char = line.len() as u32;
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
                    end: Position {
                        line: end as u32,
                        character: 0,
                    },
                },
                new_text: String::new(),
            });
        }
    }

    edits
}

/// Get the location from a `Block` enum variant.
fn block_location(block: &Block) -> &Location {
    match block {
        Block::Section(s) => &s.location,
        Block::Paragraph(p) => &p.location,
        Block::UnorderedList(l) => &l.location,
        Block::OrderedList(l) => &l.location,
        Block::DescriptionList(l) => &l.location,
        Block::CalloutList(l) => &l.location,
        Block::DelimitedBlock(d) => &d.location,
        Block::Admonition(a) => &a.location,
        Block::TableOfContents(t) => &t.location,
        Block::DiscreteHeader(h) => &h.location,
        Block::DocumentAttribute(a) => &a.location,
        Block::ThematicBreak(tb) => &tb.location,
        Block::PageBreak(pb) => &pb.location,
        Block::Image(i) => &i.location,
        Block::Audio(a) => &a.location,
        Block::Video(v) => &v.location,
        Block::Comment(c) => &c.location,
        // non_exhaustive: default location for unknown variants
        _ => {
            static DEFAULT: std::sync::LazyLock<Location> =
                std::sync::LazyLock::new(Location::default);
            &DEFAULT
        }
    }
}

/// Generate edits to ensure a blank line between consecutive top-level blocks.
#[allow(clippy::cast_possible_truncation)]
fn ensure_block_separation(
    lines: &[&str],
    ast: &Document,
    range: ops::Range<usize>,
) -> Vec<TextEdit> {
    let mut edits = Vec::new();
    let blocks = &ast.blocks;

    for pair in blocks.windows(2) {
        let (Some(prev_block), Some(curr_block)) = (pair.first(), pair.get(1)) else {
            continue;
        };

        // Convert 1-indexed AST locations to 0-indexed
        let prev_end_line = block_location(prev_block).end.line.saturating_sub(1);
        let curr_start_line = block_location(curr_block).start.line.saturating_sub(1);

        // Only process blocks within our range
        if prev_end_line < range.start || curr_start_line >= range.end {
            continue;
        }

        // If blocks are immediately adjacent (no blank line between them)
        if curr_start_line == prev_end_line + 1 {
            // Don't insert between consecutive document attributes
            if matches!(prev_block, Block::DocumentAttribute(_))
                && matches!(curr_block, Block::DocumentAttribute(_))
            {
                continue;
            }

            // Insert a blank line at the end of the previous block's last line
            if let Some(line) = lines.get(prev_end_line) {
                let col = line.len() as u32;
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
                    new_text: "\n".to_string(),
                });
            }
        }
    }

    edits
}

/// Generate edits to normalize the final newline.
#[allow(clippy::cast_possible_truncation)]
fn normalize_final_newline(text: &str, options: &FormattingOptions) -> Vec<TextEdit> {
    let mut edits = Vec::new();

    let insert_final = options.insert_final_newline.unwrap_or(true);
    let trim_final = options.trim_final_newlines.unwrap_or(true);

    if text.is_empty() {
        if insert_final {
            edits.push(TextEdit {
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 0,
                    },
                },
                new_text: "\n".to_string(),
            });
        }
        return edits;
    }

    // Count trailing newlines
    let content_end = text.trim_end_matches('\n').trim_end_matches('\r');
    let trailing = &text[content_end.len()..];
    let trailing_newline_count = trailing.matches('\n').count();

    if trim_final && trailing_newline_count > 1 {
        let lines: Vec<&str> = text.lines().collect();
        let total_lines = lines.len();

        // Find the last non-empty line
        let mut last_content_line = total_lines.saturating_sub(1);
        while last_content_line > 0 {
            if let Some(line) = lines.get(last_content_line)
                && !line.trim().is_empty()
            {
                break;
            }
            last_content_line -= 1;
        }

        // Replace from end of last content line + 1 newline to end of file
        // Keep exactly one newline after last content line
        let replace_start_line = last_content_line + 1;
        if replace_start_line < total_lines || text.ends_with('\n') {
            edits.push(TextEdit {
                range: Range {
                    start: Position {
                        line: replace_start_line as u32,
                        character: 0,
                    },
                    end: Position {
                        line: (total_lines + trailing_newline_count - 1) as u32,
                        character: 0,
                    },
                },
                new_text: String::new(),
            });
        }
    } else if insert_final && trailing_newline_count == 0 {
        let lines: Vec<&str> = text.lines().collect();
        let last_line = lines.len().saturating_sub(1);
        let last_col = lines.last().map_or(0, |l| l.len()) as u32;

        edits.push(TextEdit {
            range: Range {
                start: Position {
                    line: last_line as u32,
                    character: last_col,
                },
                end: Position {
                    line: last_line as u32,
                    character: last_col,
                },
            },
            new_text: "\n".to_string(),
        });
    }

    edits
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use tower_lsp::lsp_types::Url;

    use super::*;
    use crate::state::Workspace;

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
        let uri = Url::parse("file:///test.adoc").unwrap();
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
            let start_offset = position_to_offset(&result, edit.range.start);
            let end_offset = position_to_offset(&result, edit.range.end);
            result.replace_range(start_offset..end_offset, &edit.new_text);
        }

        result
    }

    /// Convert an LSP Position to a byte offset in the source text.
    #[allow(clippy::cast_possible_truncation)]
    fn position_to_offset(text: &str, pos: Position) -> usize {
        let mut offset = 0;
        for (i, line) in text.split('\n').enumerate() {
            if i == pos.line as usize {
                return offset + (pos.character as usize).min(line.len());
            }
            offset += line.len() + 1; // +1 for the '\n'
        }
        text.len()
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
        let uri = Url::parse("file:///test.adoc").unwrap();
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
        let uri = Url::parse("file:///test.adoc").unwrap();
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
        let uri = Url::parse("file:///test.adoc").unwrap();
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
                character: 12,
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
        let doc = DocumentState {
            text: src.to_string(),
            version: 1,
            ast: None,
            diagnostics: vec![],
            anchors: std::collections::HashMap::new(),
            xrefs: vec![],
            includes: vec![],
            attribute_refs: vec![],
            attribute_defs: vec![],
            media_sources: vec![],
        };

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
