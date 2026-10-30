//! Selection Range: smart expand/shrink selection based on AST structure
//!
//! Implements `textDocument/selectionRange` — given cursor positions, returns
//! nested selection ranges that expand through the AST hierarchy:
//! inline leaf → inline formatting → block → section → document.

use acdc_parser::{Block, DelimitedBlockType, InlineNode};
use tower_lsp_server::ls_types::{Position, Range, SelectionRange};

use crate::{
    convert::{
        LineIndex, byte_range_to_utf16_range, is_primary_location, position_in_location,
        position_to_scalar_position,
    },
    state::DocumentState,
};

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
    let Some(ast) = doc.ast() else {
        return positions.iter().map(|p| fallback_range(*p)).collect();
    };
    let ast = ast.document();

    let Some(doc_range) = byte_range_to_utf16_range(doc.text(), 0..doc.text().len()) else {
        return positions.iter().map(|p| fallback_range(*p)).collect();
    };
    let source = LineIndex::new(doc.text());

    positions
        .iter()
        .map(|&pos| {
            let Some(position) = position_to_scalar_position(doc.text(), pos) else {
                return fallback_range(pos);
            };

            let mut ranges = vec![doc_range];
            collect_block_ranges(&ast.blocks, &position, &source, &mut ranges);

            build_selection_range_chain(&ranges).unwrap_or_else(|| fallback_range(pos))
        })
        .collect()
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

fn push_range(
    location: &acdc_parser::Location,
    position: &acdc_parser::Position,
    source: &LineIndex<'_>,
    ranges: &mut Vec<Range>,
) {
    if position_in_location(position, location)
        && let Some(range) = source.range(location)
        && ranges
            .last()
            .is_none_or(|parent| parent.start <= range.start && range.end <= parent.end)
    {
        ranges.push(range);
    }
}

/// Recursively collect ranges from blocks containing the original-source position.
fn collect_block_ranges(
    blocks: &[Block],
    position: &acdc_parser::Position,
    source: &LineIndex<'_>,
    ranges: &mut Vec<Range>,
) {
    for block in blocks {
        collect_block_range(block, position, source, ranges);
    }
}

#[allow(clippy::too_many_lines)]
fn collect_block_range(
    block: &Block,
    position: &acdc_parser::Position,
    source: &LineIndex<'_>,
    ranges: &mut Vec<Range>,
) {
    // A foreign or mixed-source container can contain primary-input children.
    if is_primary_location(block.location()) && !position_in_location(position, block.location()) {
        return;
    }
    match block {
        Block::Section(section) => {
            push_range(&section.location, position, source, ranges);
            collect_block_ranges(&section.content, position, source, ranges);
        }
        Block::Paragraph(para) => {
            push_range(&para.location, position, source, ranges);
            collect_inline_ranges(&para.content, position, source, ranges);
        }
        Block::DelimitedBlock(delimited) => {
            push_range(&delimited.location, position, source, ranges);
            collect_delimited_ranges(&delimited.inner, position, source, ranges);
        }
        Block::UnorderedList(list) => {
            push_range(&list.location, position, source, ranges);
            for item in &list.items {
                push_range(&item.location, position, source, ranges);
                collect_inline_ranges(&item.principal, position, source, ranges);
                collect_block_ranges(&item.blocks, position, source, ranges);
            }
        }
        Block::OrderedList(list) => {
            push_range(&list.location, position, source, ranges);
            for item in &list.items {
                push_range(&item.location, position, source, ranges);
                collect_inline_ranges(&item.principal, position, source, ranges);
                collect_block_ranges(&item.blocks, position, source, ranges);
            }
        }
        Block::DescriptionList(list) => {
            push_range(&list.location, position, source, ranges);
            for item in &list.items {
                push_range(&item.location, position, source, ranges);
                collect_inline_ranges(&item.principal_text, position, source, ranges);
                collect_block_ranges(&item.description, position, source, ranges);
            }
        }
        Block::CalloutList(list) => {
            push_range(&list.location, position, source, ranges);
            for item in &list.items {
                push_range(&item.location, position, source, ranges);
                collect_inline_ranges(&item.principal, position, source, ranges);
                collect_block_ranges(&item.blocks, position, source, ranges);
            }
        }
        Block::Admonition(adm) => {
            push_range(&adm.location, position, source, ranges);
            collect_block_ranges(&adm.blocks, position, source, ranges);
        }
        Block::DiscreteHeader(h) => {
            push_range(&h.location, position, source, ranges);
        }
        Block::DocumentAttribute(a) => {
            push_range(&a.location, position, source, ranges);
        }
        Block::Image(i) => {
            push_range(&i.location, position, source, ranges);
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

fn collect_delimited_ranges(
    inner: &DelimitedBlockType,
    position: &acdc_parser::Position,
    source: &LineIndex<'_>,
    ranges: &mut Vec<Range>,
) {
    match inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => {
            collect_block_ranges(blocks, position, source, ranges);
        }
        DelimitedBlockType::DelimitedListing(inlines)
        | DelimitedBlockType::DelimitedLiteral(inlines)
        | DelimitedBlockType::DelimitedPass(inlines)
        | DelimitedBlockType::DelimitedVerse(inlines)
        | DelimitedBlockType::DelimitedComment(inlines) => {
            collect_inline_ranges(inlines, position, source, ranges);
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
                    collect_block_ranges(&col.content, position, source, ranges);
                }
            }
        }
        DelimitedBlockType::DelimitedStem(_)
        // non_exhaustive
        | _ => {}
    }
}

fn collect_inline_ranges(
    inlines: &[InlineNode],
    position: &acdc_parser::Position,
    source: &LineIndex<'_>,
    ranges: &mut Vec<Range>,
) {
    for inline in inlines {
        let loc = inline.location();
        if is_primary_location(loc) && !position_in_location(position, loc) {
            continue;
        }

        push_range(loc, position, source, ranges);

        // Recurse into container inlines
        match inline {
            InlineNode::BoldText(b) => collect_inline_ranges(&b.content, position, source, ranges),
            InlineNode::ItalicText(i) => collect_inline_ranges(&i.content, position, source, ranges),
            InlineNode::MonospaceText(m) => collect_inline_ranges(&m.content, position, source, ranges),
            InlineNode::HighlightText(h) => collect_inline_ranges(&h.content, position, source, ranges),
            InlineNode::SubscriptText(s) => collect_inline_ranges(&s.content, position, source, ranges),
            InlineNode::SuperscriptText(s) => collect_inline_ranges(&s.content, position, source, ranges),
            InlineNode::CurvedQuotationText(q) => {
                collect_inline_ranges(&q.content, position, source, ranges);
            }
            InlineNode::CurvedApostropheText(a) => {
                collect_inline_ranges(&a.content, position, source, ranges);
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
