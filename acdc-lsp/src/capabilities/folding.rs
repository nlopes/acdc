//! Folding Ranges: enable collapsible sections and blocks in editors

use acdc_parser::{Block, DelimitedBlockType, Document, Location};
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
pub(crate) fn compute_folding_ranges(doc: &Document, text: &str) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();
    let source = LineIndex::new(text);
    collect_ranges_from_blocks(&doc.blocks, &source, &mut ranges);
    ranges
}

fn collect_ranges_from_blocks(
    blocks: &[Block],
    source: &LineIndex<'_>,
    ranges: &mut Vec<FoldingRange>,
) {
    for block in blocks {
        collect_ranges_from_block(block, source, ranges);
    }
}

fn collect_ranges_from_block(
    block: &Block,
    source: &LineIndex<'_>,
    ranges: &mut Vec<FoldingRange>,
) {
    match block {
        Block::Section(section) => {
            // Section is foldable if it spans multiple lines
            if let Some(range) = make_folding_range(&section.location, FoldingRangeKind::Region, source) {
                ranges.push(range);
            }
            // Recurse into section content
            collect_ranges_from_blocks(&section.content, source, ranges);
        }
        Block::DelimitedBlock(delimited) => {
            // Determine kind based on block type
            let kind = match &delimited.inner {
                DelimitedBlockType::DelimitedComment(_) => FoldingRangeKind::Comment,
                DelimitedBlockType::DelimitedExample(_)
                | DelimitedBlockType::DelimitedOpen(_)
                | DelimitedBlockType::DelimitedSidebar(_)
                | DelimitedBlockType::DelimitedQuote(_)
                | DelimitedBlockType::DelimitedListing(_)
                | DelimitedBlockType::DelimitedLiteral(_)
                | DelimitedBlockType::DelimitedPass(_)
                | DelimitedBlockType::DelimitedVerse(_)
                | DelimitedBlockType::DelimitedTable(_)
                | DelimitedBlockType::DelimitedStem(_)
                // non_exhaustive
                | _ => FoldingRangeKind::Region,
            };
            if let Some(range) = make_folding_range(&delimited.location, kind, source) {
                ranges.push(range);
            }
            // Recurse into nested content
            collect_ranges_from_delimited(&delimited.inner, source, ranges);
        }
        Block::UnorderedList(list) => {
            if let Some(range) = make_folding_range(&list.location, FoldingRangeKind::Region, source) {
                ranges.push(range);
            }
            for item in &list.items {
                collect_ranges_from_blocks(&item.blocks, source, ranges);
            }
        }
        Block::OrderedList(list) => {
            if let Some(range) = make_folding_range(&list.location, FoldingRangeKind::Region, source) {
                ranges.push(range);
            }
            for item in &list.items {
                collect_ranges_from_blocks(&item.blocks, source, ranges);
            }
        }
        Block::DescriptionList(list) => {
            if let Some(range) = make_folding_range(&list.location, FoldingRangeKind::Region, source) {
                ranges.push(range);
            }
            for item in &list.items {
                collect_ranges_from_blocks(&item.description, source, ranges);
            }
        }
        Block::Admonition(adm) => {
            if let Some(range) = make_folding_range(&adm.location, FoldingRangeKind::Region, source) {
                ranges.push(range);
            }
            collect_ranges_from_blocks(&adm.blocks, source, ranges);
        }
        Block::Comment(comment) => {
            if let Some(range) = make_folding_range(&comment.location, FoldingRangeKind::Comment, source) {
                ranges.push(range);
            }
        }
        Block::TableOfContents(_)
        | Block::DiscreteHeader(_)
        | Block::DocumentAttribute(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::CalloutList(_)
        | Block::Paragraph(_)
        | Block::Image(_)
        | Block::Audio(_)
        | Block::Video(_)
        // non_exhaustive
        | _ => {}
    }
}

fn collect_ranges_from_delimited(
    inner: &DelimitedBlockType,
    source: &LineIndex<'_>,
    ranges: &mut Vec<FoldingRange>,
) {
    match inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => {
            collect_ranges_from_blocks(blocks, source, ranges);
        }
        DelimitedBlockType::DelimitedListing(_)
        | DelimitedBlockType::DelimitedLiteral(_)
        | DelimitedBlockType::DelimitedPass(_)
        | DelimitedBlockType::DelimitedVerse(_)
        | DelimitedBlockType::DelimitedComment(_)
        | DelimitedBlockType::DelimitedTable(_)
        | DelimitedBlockType::DelimitedStem(_)
        // non_exhaustive
        | _ => {}
    }
}

/// Create a folding range if the location spans multiple lines
fn make_folding_range(
    loc: &Location,
    kind: FoldingRangeKind,
    source: &LineIndex<'_>,
) -> Option<FoldingRange> {
    if !is_primary_location(loc) {
        return None;
    }
    let range = source.range(loc)?;
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
    use acdc_parser::Options;

    #[test]
    fn folds_validate_inclusive_source_endpoints_and_omit_columns()
    -> Result<(), Box<dyn std::error::Error>> {
        let source = LineIndex::new("é😀\r\nnext\r\n");
        let mut span = Location::point(acdc_parser::Position::new(1, 1));
        span.end = acdc_parser::Position::new(2, 6);
        let fold = make_folding_range(&span, FoldingRangeKind::Region, &source).ok_or("fold")?;
        assert_eq!((fold.start_line, fold.end_line), (0, 1));
        assert_eq!((fold.start_character, fold.end_character), (None, None));
        for end in [(2, 7), (9, 1), (0, 0)] {
            span.end = acdc_parser::Position::new(end.0, end.1);
            assert!(make_folding_range(&span, FoldingRangeKind::Region, &source).is_none());
        }
        span.end = acdc_parser::Position::new(2, 4);
        span.end.file = Some(std::sync::Arc::new(vec!["included.adoc".into()]));
        assert!(make_folding_range(&span, FoldingRangeKind::Region, &source).is_none());
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

        let ranges = compute_folding_ranges(parsed.document(), content);
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

        let ranges = compute_folding_ranges(parsed.document(), content);
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

        let ranges = compute_folding_ranges(parsed.document(), content);
        // Should have 1 folding range for the open block
        assert_eq!(ranges.len(), 1);
        assert_eq!(
            ranges.first().map(|r| r.kind.clone()),
            Some(Some(FoldingRangeKind::Region))
        );
        Ok(())
    }
}
