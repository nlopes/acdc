//! Selection Range: smart expand/shrink selection based on AST structure
//!
//! Implements `textDocument/selectionRange` — given cursor positions, returns
//! nested selection ranges that expand through the AST hierarchy:
//! inline leaf → inline formatting → block → section → document.

use acdc_parser::{Block, DelimitedBlockType, InlineNode};
use tower_lsp_server::ls_types::{Position, Range, SelectionRange};

use crate::convert::{location_to_range, offset_in_location, position_to_offset, to_lsp_u32};
use crate::state::DocumentState;

/// Compute selection ranges for the given positions.
///
/// For each position, returns a `SelectionRange` representing the innermost
/// syntactic element, with `parent` links expanding outward through the AST
/// hierarchy.
#[must_use]
pub(crate) fn compute_selection_ranges(
    doc: &DocumentState,
    positions: &[Position],
) -> Vec<SelectionRange> {
    let Some(ast) = doc.ast.as_ref() else {
        return positions.iter().map(|p| fallback_range(*p)).collect();
    };

    let doc_range = document_range(&doc.text);

    positions
        .iter()
        .map(|&pos| {
            let Some(offset) = position_to_offset(&doc.text, pos) else {
                return fallback_range(pos);
            };

            let mut ranges = vec![doc_range];
            collect_block_ranges(&ast.blocks, offset, &mut ranges);

            build_selection_range_chain(&ranges).unwrap_or_else(|| fallback_range(pos))
        })
        .collect()
}

/// Compute the range covering the entire document text.
fn document_range(text: &str) -> Range {
    let lines: Vec<&str> = text.lines().collect();
    let last_line = lines.len().saturating_sub(1);
    let last_char = lines.last().map_or(0, |l| l.chars().count());

    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: to_lsp_u32(last_line),
            character: to_lsp_u32(last_char),
        },
    }
}

/// Build a `SelectionRange` chain from outermost-to-innermost ranges.
///
/// The returned `SelectionRange` is the innermost, with `.parent` pointing
/// to progressively larger scopes.
fn build_selection_range_chain(ranges: &[Range]) -> Option<SelectionRange> {
    let mut current: Option<SelectionRange> = None;
    for range in ranges {
        current = Some(SelectionRange {
            range: *range,
            parent: current.map(Box::new),
        });
    }
    current
}

/// Fallback: zero-width range at the position with no parent.
fn fallback_range(pos: Position) -> SelectionRange {
    SelectionRange {
        range: Range {
            start: pos,
            end: pos,
        },
        parent: None,
    }
}

/// Recursively collect ranges from blocks containing the offset.
fn collect_block_ranges(blocks: &[Block], offset: usize, ranges: &mut Vec<Range>) {
    for block in blocks {
        collect_block_range(block, offset, ranges);
    }
}

#[allow(clippy::too_many_lines)]
fn collect_block_range(block: &Block, offset: usize, ranges: &mut Vec<Range>) {
    match block {
        Block::Section(section) => {
            if !offset_in_location(offset, &section.location) {
                return;
            }
            ranges.push(location_to_range(&section.location));
            collect_block_ranges(&section.content, offset, ranges);
        }
        Block::Paragraph(para) => {
            if !offset_in_location(offset, &para.location) {
                return;
            }
            ranges.push(location_to_range(&para.location));
            collect_inline_ranges(&para.content, offset, ranges);
        }
        Block::DelimitedBlock(delimited) => {
            if !offset_in_location(offset, &delimited.location) {
                return;
            }
            ranges.push(location_to_range(&delimited.location));
            collect_delimited_ranges(&delimited.inner, offset, ranges);
        }
        Block::UnorderedList(list) => {
            if !offset_in_location(offset, &list.location) {
                return;
            }
            ranges.push(location_to_range(&list.location));
            for item in &list.items {
                if offset_in_location(offset, &item.location) {
                    ranges.push(location_to_range(&item.location));
                    collect_inline_ranges(&item.principal, offset, ranges);
                    collect_block_ranges(&item.blocks, offset, ranges);
                }
            }
        }
        Block::OrderedList(list) => {
            if !offset_in_location(offset, &list.location) {
                return;
            }
            ranges.push(location_to_range(&list.location));
            for item in &list.items {
                if offset_in_location(offset, &item.location) {
                    ranges.push(location_to_range(&item.location));
                    collect_inline_ranges(&item.principal, offset, ranges);
                    collect_block_ranges(&item.blocks, offset, ranges);
                }
            }
        }
        Block::DescriptionList(list) => {
            if !offset_in_location(offset, &list.location) {
                return;
            }
            ranges.push(location_to_range(&list.location));
            for item in &list.items {
                if offset_in_location(offset, &item.location) {
                    ranges.push(location_to_range(&item.location));
                    collect_inline_ranges(&item.principal_text, offset, ranges);
                    collect_block_ranges(&item.description, offset, ranges);
                }
            }
        }
        Block::CalloutList(list) => {
            if !offset_in_location(offset, &list.location) {
                return;
            }
            ranges.push(location_to_range(&list.location));
            for item in &list.items {
                if offset_in_location(offset, &item.location) {
                    ranges.push(location_to_range(&item.location));
                    collect_inline_ranges(&item.principal, offset, ranges);
                    collect_block_ranges(&item.blocks, offset, ranges);
                }
            }
        }
        Block::Admonition(adm) => {
            if !offset_in_location(offset, &adm.location) {
                return;
            }
            ranges.push(location_to_range(&adm.location));
            collect_block_ranges(&adm.blocks, offset, ranges);
        }
        Block::DiscreteHeader(h) => {
            if offset_in_location(offset, &h.location) {
                ranges.push(location_to_range(&h.location));
            }
        }
        Block::DocumentAttribute(a) => {
            if offset_in_location(offset, &a.location) {
                ranges.push(location_to_range(&a.location));
            }
        }
        Block::Image(i) => {
            if offset_in_location(offset, &i.location) {
                ranges.push(location_to_range(&i.location));
            }
        }
        Block::TableOfContents(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::Audio(_)
        | Block::Video(_)
        | Block::Comment(_)
        // non_exhaustive
        | _ => {}
    }
}

fn collect_delimited_ranges(inner: &DelimitedBlockType, offset: usize, ranges: &mut Vec<Range>) {
    match inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => {
            collect_block_ranges(blocks, offset, ranges);
        }
        DelimitedBlockType::DelimitedListing(inlines)
        | DelimitedBlockType::DelimitedLiteral(inlines)
        | DelimitedBlockType::DelimitedPass(inlines)
        | DelimitedBlockType::DelimitedVerse(inlines)
        | DelimitedBlockType::DelimitedComment(inlines) => {
            collect_inline_ranges(inlines, offset, ranges);
        }
        DelimitedBlockType::DelimitedTable(table) => {
            // TableRow/TableColumn don't have Location fields, so we
            // recurse directly into cell content blocks.
            for row in table
                .header
                .iter()
                .chain(table.rows.iter())
                .chain(table.footer.iter())
            {
                for col in &row.columns {
                    collect_block_ranges(&col.content, offset, ranges);
                }
            }
        }
        DelimitedBlockType::DelimitedStem(_)
        // non_exhaustive
        | _ => {}
    }
}

fn collect_inline_ranges(inlines: &[InlineNode], offset: usize, ranges: &mut Vec<Range>) {
    for inline in inlines {
        let loc = inline.location();
        if !offset_in_location(offset, loc) {
            continue;
        }

        ranges.push(location_to_range(loc));

        // Recurse into container inlines
        match inline {
            InlineNode::BoldText(b) => collect_inline_ranges(&b.content, offset, ranges),
            InlineNode::ItalicText(i) => collect_inline_ranges(&i.content, offset, ranges),
            InlineNode::MonospaceText(m) => collect_inline_ranges(&m.content, offset, ranges),
            InlineNode::HighlightText(h) => collect_inline_ranges(&h.content, offset, ranges),
            InlineNode::SubscriptText(s) => collect_inline_ranges(&s.content, offset, ranges),
            InlineNode::SuperscriptText(s) => collect_inline_ranges(&s.content, offset, ranges),
            InlineNode::CurvedQuotationText(q) => {
                collect_inline_ranges(&q.content, offset, ranges);
            }
            InlineNode::CurvedApostropheText(a) => {
                collect_inline_ranges(&a.content, offset, ranges);
            }
            // Leaf inlines: no children to recurse into
            InlineNode::PlainText(_)
            | InlineNode::RawText(_)
            | InlineNode::VerbatimText(_)
            | InlineNode::StandaloneCurvedApostrophe(_)
            | InlineNode::LineBreak(_)
            | InlineNode::InlineAnchor(_)
            | InlineNode::Macro(_)
            | InlineNode::CalloutRef(_)
            // non_exhaustive
            | _ => {}
        }
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::state::Workspace;
    use tower_lsp_server::ls_types::Uri;

    /// Helper: parse a document and compute selection ranges for a position.
    fn selection_ranges_at(content: &str, line: u32, character: u32) -> Vec<SelectionRange> {
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>().unwrap();
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).unwrap();
        let positions = vec![Position { line, character }];
        compute_selection_ranges(&doc, &positions)
    }

    /// Walk the parent chain and collect all ranges.
    fn collect_chain(sr: &SelectionRange) -> Vec<Range> {
        let mut result = vec![sr.range];
        let mut current = &sr.parent;
        while let Some(parent) = current {
            result.push(parent.range);
            current = &parent.parent;
        }
        result
    }

    #[test]
    fn test_basic_section_selection() {
        let content = "= Document Title\n\n== Section\n\nSome text here.\n";
        // "text" is on line 4 (0-indexed), character 5
        let results = selection_ranges_at(content, 4, 5);
        assert_eq!(results.len(), 1);

        let chain = collect_chain(&results[0]);
        // Should have at least: text node, paragraph, section, document
        assert!(
            chain.len() >= 3,
            "Expected at least 3 levels, got {}: {chain:?}",
            chain.len()
        );

        // Innermost should be contained in outermost
        let innermost = chain[0];
        let outermost = chain.last().unwrap();
        assert!(innermost.start.line >= outermost.start.line);
    }

    #[test]
    fn test_nested_inline_markup() {
        let content = "= Title\n\n*_bold italic_*\n";
        let results = selection_ranges_at(content, 2, 3);
        assert_eq!(results.len(), 1);

        let chain = collect_chain(&results[0]);
        // Should have: plain text, italic, bold, paragraph, document
        assert!(
            chain.len() >= 4,
            "Expected at least 4 levels for nested inline, got {}: {chain:?}",
            chain.len()
        );
    }

    #[test]
    fn test_list_item_selection() {
        let content = "= Title\n\n== Section\n\n* First item\n* Second item\n";
        let results = selection_ranges_at(content, 5, 3);
        assert_eq!(results.len(), 1);

        let chain = collect_chain(&results[0]);
        // Should have: text, list item, list, section, document
        assert!(
            chain.len() >= 4,
            "Expected at least 4 levels for list item, got {}: {chain:?}",
            chain.len()
        );
    }

    #[test]
    fn test_multiple_positions() {
        let content = "= Title\n\nFirst paragraph.\n\nSecond paragraph.\n";
        let workspace = Workspace::new();
        let uri = "file:///test.adoc".parse::<Uri>().unwrap();
        workspace.update_document(uri.clone(), content.to_string(), 1);
        let doc = workspace.get_document(&uri).unwrap();
        let positions = vec![
            Position {
                line: 2,
                character: 0,
            },
            Position {
                line: 4,
                character: 0,
            },
        ];
        let results = compute_selection_ranges(&doc, &positions);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_no_ast_fallback() {
        let doc = DocumentState::new_failure(String::new(), 1, vec![]);
        let positions = vec![Position {
            line: 0,
            character: 0,
        }];
        let results = compute_selection_ranges(&doc, &positions);
        assert_eq!(results.len(), 1);
        assert!(results[0].parent.is_none());
    }

    #[test]
    fn test_chain_is_strictly_expanding() {
        let content = "= Title\n\n== Section\n\n*bold text*\n";
        let results = selection_ranges_at(content, 4, 3);
        assert_eq!(results.len(), 1);

        let chain = collect_chain(&results[0]);
        // Each range should be contained within its parent
        for window in chain.windows(2) {
            let inner = window[0];
            let outer = window[1];
            assert!(
                inner.start.line > outer.start.line
                    || (inner.start.line == outer.start.line
                        && inner.start.character >= outer.start.character),
                "Inner start {inner:?} should be >= outer start {outer:?}"
            );
            assert!(
                inner.end.line < outer.end.line
                    || (inner.end.line == outer.end.line
                        && inner.end.character <= outer.end.character),
                "Inner end {inner:?} should be <= outer end {outer:?}"
            );
        }
    }
}
