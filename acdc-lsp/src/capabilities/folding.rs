//! Folding Ranges: enable collapsible sections and blocks in editors

use acdc_parser::ast::{BlockKind, DocumentRef, LocationRef, StructuralRef};
use tower_lsp_server::ls_types::{FoldingRange, FoldingRangeKind};

use crate::convert::{LineIndex, is_primary_location};

/// Compute all folding ranges in a document
///
/// Returns ranges for:
/// - Sections (collapsible by level)
/// - Delimited blocks (listing, example, sidebar, etc.)
/// - Lists (ordered, unordered, description)
/// - Comment blocks
#[must_use]
pub(crate) fn compute_folding_ranges(doc: DocumentRef<'_>, text: &str) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();
    let source = LineIndex::new(text);
    for block in doc.blocks() {
        collect_ranges(block.into(), &source, &mut ranges);
    }
    ranges
}

fn collect_ranges(node: StructuralRef<'_>, source: &LineIndex<'_>, ranges: &mut Vec<FoldingRange>) {
    match node {
        StructuralRef::Block(block) => {
            let (kind, descend) = match block.kind() {
                BlockKind::Section(_)
                | BlockKind::Admonition(_)
                | BlockKind::Example(_)
                | BlockKind::Open(_)
                | BlockKind::Sidebar(_)
                | BlockKind::Quote(_)
                | BlockKind::UnorderedList(_)
                | BlockKind::OrderedList(_)
                | BlockKind::DescriptionList(_) => (FoldingRangeKind::Region, true),
                BlockKind::Listing(_)
                | BlockKind::Literal(_)
                | BlockKind::Passthrough(_)
                | BlockKind::Verse(_)
                | BlockKind::Table(_)
                | BlockKind::Stem(_) => (FoldingRangeKind::Region, false),
                BlockKind::Comment(_) | BlockKind::DelimitedComment(_) => {
                    (FoldingRangeKind::Comment, false)
                }
                BlockKind::TableOfContents(_)
                | BlockKind::DiscreteHeader(_)
                | BlockKind::DocumentAttribute(_)
                | BlockKind::ThematicBreak(_)
                | BlockKind::PageBreak(_)
                | BlockKind::CalloutList(_)
                | BlockKind::Paragraph(_)
                | BlockKind::Image(_)
                | BlockKind::Audio(_)
                | BlockKind::Video(_)
                | _ => return,
            };
            if let Some(range) = make_folding_range(block.location(), kind, source) {
                ranges.push(range);
            }
            if !descend {
                return;
            }
        }
        StructuralRef::ListItem(_) | StructuralRef::DescriptionListItem(_) => {}
        StructuralRef::Document(_)
        | StructuralRef::Header(_)
        | StructuralRef::CalloutListItem(_)
        | StructuralRef::TableRow(_)
        | StructuralRef::TableCell(_)
        | _ => return,
    }
    for child in node.children() {
        collect_ranges(child, source, ranges);
    }
}

/// Create a folding range if the location spans multiple lines
fn make_folding_range(
    loc: LocationRef<'_>,
    kind: FoldingRangeKind,
    source: &LineIndex<'_>,
) -> Option<FoldingRange> {
    if !is_primary_location(loc) {
        return None;
    }
    let range = source.range(&loc.into())?;
    let end_line = if range.end.character == 0 {
        range.end.line.saturating_sub(1)
    } else {
        range.end.line
    };
    (end_line > range.start.line).then_some(FoldingRange {
        start_line: range.start.line,
        start_character: None,
        end_line,
        end_character: None,
        kind: Some(kind),
        collapsed_text: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::{Location, Options};

    #[test]
    fn folds_validate_inclusive_source_endpoints_and_omit_columns()
    -> Result<(), Box<dyn std::error::Error>> {
        let source = LineIndex::new("é😀\r\nnext\r\n");
        let mut span = Location::point(acdc_parser::Position::new(1, 1));
        span.end = acdc_parser::Position::new(2, 6);
        let fold = make_folding_range(LocationRef::from(&span), FoldingRangeKind::Region, &source)
            .ok_or("fold")?;
        assert_eq!((fold.start_line, fold.end_line), (0, 1));
        assert_eq!((fold.start_character, fold.end_character), (None, None));
        for end in [(2, 7), (9, 1), (0, 0)] {
            span.end = acdc_parser::Position::new(end.0, end.1);
            assert!(
                make_folding_range(LocationRef::from(&span), FoldingRangeKind::Region, &source)
                    .is_none()
            );
        }
        span.end = acdc_parser::Position::new(2, 4);
        span.end.file = Some(std::sync::Arc::new(vec!["included.adoc".into()]));
        assert!(
            make_folding_range(LocationRef::from(&span), FoldingRangeKind::Region, &source)
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn test_section_folding() -> Result<(), acdc_parser::Error> {
        let content = r"= Document Title

== First Section

Some content here.

More content.

== Second Section

Different content.
";
        let options = Options::default();
        let parsed = acdc_parser::parse(content, &options)?;

        let ranges = compute_folding_ranges(parsed.document_ref(), content);
        // Should have 2 folding ranges for the two sections
        assert_eq!(ranges.len(), 2);
        Ok(())
    }

    #[test]
    fn test_delimited_block_folding() -> Result<(), acdc_parser::Error> {
        let content = r#"= Document

[source,rust]
----
fn main() {
    println!("Hello");
}
----
"#;
        let options = Options::default();
        let parsed = acdc_parser::parse(content, &options)?;

        let ranges = compute_folding_ranges(parsed.document_ref(), content);
        // Should have 1 folding range for the listing block
        assert_eq!(ranges.len(), 1);
        assert_eq!(
            ranges.first().map(|r| r.kind.clone()),
            Some(Some(FoldingRangeKind::Region))
        );
        Ok(())
    }

    #[test]
    fn test_open_block_folding() -> Result<(), acdc_parser::Error> {
        // Test that open blocks are foldable
        let content = r"= Document

[NOTE]
--
This is a
multi-line
block
--
";
        let options = Options::default();
        let parsed = acdc_parser::parse(content, &options)?;

        let ranges = compute_folding_ranges(parsed.document_ref(), content);
        // Should have 1 folding range for the open block
        assert_eq!(ranges.len(), 1);
        assert_eq!(
            ranges.first().map(|r| r.kind.clone()),
            Some(Some(FoldingRangeKind::Region))
        );
        Ok(())
    }
}
