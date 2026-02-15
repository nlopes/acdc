//! Folding Ranges: enable collapsible sections and blocks in editors

use acdc_parser::{Block, DelimitedBlockType, Document, Location};
use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};

/// Convert usize to u32 for LSP types, saturating at `u32::MAX`.
fn to_lsp_u32(val: usize) -> u32 {
    val.try_into().unwrap_or(u32::MAX)
}

/// Compute all folding ranges in a document
///
/// Returns ranges for:
/// - Sections (collapsible by level)
/// - Delimited blocks (listing, example, sidebar, etc.)
/// - Lists (ordered, unordered, description)
/// - Comment blocks
#[must_use]
pub fn compute_folding_ranges(doc: &Document) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();
    collect_ranges_from_blocks(&doc.blocks, &mut ranges);
    ranges
}

fn collect_ranges_from_blocks(blocks: &[Block], ranges: &mut Vec<FoldingRange>) {
    for block in blocks {
        collect_ranges_from_block(block, ranges);
    }
}

fn collect_ranges_from_block(block: &Block, ranges: &mut Vec<FoldingRange>) {
    match block {
        Block::Section(section) => {
            // Section is foldable if it spans multiple lines
            if let Some(range) = make_folding_range(&section.location, FoldingRangeKind::Region) {
                ranges.push(range);
            }
            // Recurse into section content
            collect_ranges_from_blocks(&section.content, ranges);
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
            if let Some(range) = make_folding_range(&delimited.location, kind) {
                ranges.push(range);
            }
            // Recurse into nested content
            collect_ranges_from_delimited(&delimited.inner, ranges);
        }
        Block::UnorderedList(list) => {
            if let Some(range) = make_folding_range(&list.location, FoldingRangeKind::Region) {
                ranges.push(range);
            }
            for item in &list.items {
                collect_ranges_from_blocks(&item.blocks, ranges);
            }
        }
        Block::OrderedList(list) => {
            if let Some(range) = make_folding_range(&list.location, FoldingRangeKind::Region) {
                ranges.push(range);
            }
            for item in &list.items {
                collect_ranges_from_blocks(&item.blocks, ranges);
            }
        }
        Block::DescriptionList(list) => {
            if let Some(range) = make_folding_range(&list.location, FoldingRangeKind::Region) {
                ranges.push(range);
            }
            for item in &list.items {
                collect_ranges_from_blocks(&item.description, ranges);
            }
        }
        Block::Admonition(adm) => {
            if let Some(range) = make_folding_range(&adm.location, FoldingRangeKind::Region) {
                ranges.push(range);
            }
            collect_ranges_from_blocks(&adm.blocks, ranges);
        }
        Block::Comment(comment) => {
            if let Some(range) = make_folding_range(&comment.location, FoldingRangeKind::Comment) {
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

fn collect_ranges_from_delimited(inner: &DelimitedBlockType, ranges: &mut Vec<FoldingRange>) {
    match inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => {
            collect_ranges_from_blocks(blocks, ranges);
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
fn make_folding_range(loc: &Location, kind: FoldingRangeKind) -> Option<FoldingRange> {
    // Only create folding range if it spans at least 2 lines
    if loc.end.line > loc.start.line {
        Some(FoldingRange {
            start_line: to_lsp_u32(loc.start.line.saturating_sub(1)),
            start_character: None,
            end_line: to_lsp_u32(loc.end.line.saturating_sub(1)),
            end_character: None,
            kind: Some(kind),
            collapsed_text: None,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::Options;

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
        let doc = acdc_parser::parse(content, &options)?;

        let ranges = compute_folding_ranges(&doc);
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
        let doc = acdc_parser::parse(content, &options)?;

        let ranges = compute_folding_ranges(&doc);
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
        let doc = acdc_parser::parse(content, &options)?;

        let ranges = compute_folding_ranges(&doc);
        // Should have 1 folding range for the open block
        assert_eq!(ranges.len(), 1);
        assert_eq!(
            ranges.first().map(|r| r.kind.clone()),
            Some(Some(FoldingRangeKind::Region))
        );
        Ok(())
    }
}
