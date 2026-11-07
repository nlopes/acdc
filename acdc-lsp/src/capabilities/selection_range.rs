//! Selection Range: smart expand/shrink selection based on AST structure
//!
//! Implements `textDocument/selectionRange` — given cursor positions, returns
//! nested selection ranges that expand through the AST hierarchy:
//! inline leaf → inline formatting → block → section → document.

use acdc_parser::ast::{
    BlockKind, ContentRef, ContentRole, InlineKind, LocationRef, StructuralRef,
};
use tower_lsp_server::ls_types::{Position, Range, SelectionRange};

use crate::{
    convert::{
        LineIndex, byte_range_to_utf16_range, is_primary_location, normalize_position,
        position_in_location, position_to_scalar_position,
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
        return positions
            .iter()
            .map(|p| fallback_range(normalize_position(doc.text(), *p).unwrap_or(*p)))
            .collect();
    };
    let ast = ast.document();

    let Some(doc_range) = byte_range_to_utf16_range(doc.text(), 0..doc.text().len()) else {
        return positions
            .iter()
            .map(|p| fallback_range(normalize_position(doc.text(), *p).unwrap_or(*p)))
            .collect();
    };
    let source = LineIndex::new(doc.text());

    positions
        .iter()
        .map(|&pos| {
            let Some(position) = position_to_scalar_position(doc.text(), pos) else {
                return fallback_range(pos);
            };

            let mut ranges = vec![doc_range];
            for block in ast.blocks() {
                collect_structural_ranges(block.into(), &position, &source, &mut ranges);
            }

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
    location: LocationRef<'_>,
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

fn collect_structural_ranges(
    node: StructuralRef<'_>,
    position: &acdc_parser::Position,
    source: &LineIndex<'_>,
    ranges: &mut Vec<Range>,
) {
    let location = match node {
        StructuralRef::Block(block) => {
            // A foreign or mixed-source container can contain primary-input children.
            if is_primary_location(block.location())
                && !position_in_location(position, block.location())
            {
                return;
            }
            match block.kind() {
                BlockKind::Section(_)
                | BlockKind::Paragraph(_)
                | BlockKind::Example(_)
                | BlockKind::Open(_)
                | BlockKind::Sidebar(_)
                | BlockKind::Quote(_)
                | BlockKind::Listing(_)
                | BlockKind::Literal(_)
                | BlockKind::Passthrough(_)
                | BlockKind::Verse(_)
                | BlockKind::DelimitedComment(_)
                | BlockKind::Table(_)
                | BlockKind::Stem(_)
                | BlockKind::UnorderedList(_)
                | BlockKind::OrderedList(_)
                | BlockKind::DescriptionList(_)
                | BlockKind::CalloutList(_)
                | BlockKind::Admonition(_)
                | BlockKind::DiscreteHeader(_)
                | BlockKind::DocumentAttribute(_)
                | BlockKind::Image(_) => Some(block.location()),
                BlockKind::TableOfContents(_)
                | BlockKind::ThematicBreak(_)
                | BlockKind::PageBreak(_)
                | BlockKind::Audio(_)
                | BlockKind::Video(_)
                | BlockKind::Comment(_)
                | _ => return,
            }
        }
        StructuralRef::ListItem(item) => Some(item.location()),
        StructuralRef::DescriptionListItem(item) => Some(item.location()),
        StructuralRef::DescriptionListTerm(term) => Some(term.location()),
        StructuralRef::DescriptionListDefinition(definition) => Some(definition.location()),
        StructuralRef::CalloutListItem(item) => Some(item.location()),
        StructuralRef::TableRow(_) | StructuralRef::TableCell(_) => None,
        StructuralRef::Document(_) | StructuralRef::Header(_) | _ => return,
    };
    if let Some(location) = location {
        push_range(location, position, source, ranges);
    }
    for field in node.content_fields() {
        if matches!(field.role(), ContentRole::Body | ContentRole::ListPrincipal) {
            collect_inline_ranges(field.content(), position, source, ranges);
        }
    }
    for child in node.children() {
        collect_structural_ranges(child, position, source, ranges);
    }
}

fn collect_inline_ranges(
    content: ContentRef<'_>,
    position: &acdc_parser::Position,
    source: &LineIndex<'_>,
    ranges: &mut Vec<Range>,
) {
    for inline in content.inlines() {
        let location = inline.location();
        if is_primary_location(location) && !position_in_location(position, location) {
            continue;
        }
        push_range(location, position, source, ranges);
        if matches!(
            inline.kind(),
            InlineKind::Bold(_)
                | InlineKind::Italic(_)
                | InlineKind::Monospace(_)
                | InlineKind::Highlight(_)
                | InlineKind::Subscript(_)
                | InlineKind::Superscript(_)
                | InlineKind::CurvedQuotation(_)
                | InlineKind::CurvedApostrophe(_)
        ) {
            for field in inline.content_fields() {
                collect_inline_ranges(field.content(), position, source, ranges);
            }
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
        let doc = DocumentState::new_failure(String::new(), 1);
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
