//! Section presentation utilities shared by converters.

use acdc_parser::{
    Block, ColumnStyle, SectionKind,
    ast::{BlockKind, BlockRef, ColumnFormatRef},
};

use crate::TraversalContext;

/// Whether the last top-level section has the requested style.
#[must_use]
pub fn last_section_has_style(blocks: &[Block<'_>], style: &str) -> bool {
    let last_section = blocks.iter().rev().find_map(|block| {
        if let Block::Section(section) = block {
            Some(section)
        } else {
            None
        }
    });
    last_section.is_some_and(|section| section.metadata.style.is_some_and(|value| value == style))
}

/// Whether the document contains an index section, including nested `AsciiDoc` cells.
///
/// Compound blocks are searched for nested documents. Section-looking text in
/// a compound block is a paragraph, so it cannot activate an index catalog.
#[must_use]
pub fn has_index_section(blocks: &[Block<'_>]) -> bool {
    contains_index_section(blocks.iter().map(BlockRef::from))
}

/// Search typed blocks for an index section, stopping at the first match.
///
/// Descends through sections, compound blocks, admonitions, list attachments,
/// and `AsciiDoc` table cells. Inline content and other cell styles are excluded.
#[must_use]
pub fn contains_index_section<'doc>(mut blocks: impl Iterator<Item = BlockRef<'doc>>) -> bool {
    blocks.any(|block| match block.kind() {
        BlockKind::Section(section) => {
            section.kind() == SectionKind::Index || contains_index_section(section.blocks())
        }
        BlockKind::Example(block) => contains_index_section(block.blocks()),
        BlockKind::Open(block) => contains_index_section(block.blocks()),
        BlockKind::Quote(block) => contains_index_section(block.blocks()),
        BlockKind::Sidebar(block) => contains_index_section(block.blocks()),
        BlockKind::Table(table) => table
            .header()
            .into_iter()
            .chain(table.rows())
            .chain(table.footer())
            .any(|row| {
                let mut columns = table.columns();
                row.cells().any(|cell| {
                    // Column defaults follow stored cell positions, even after spans.
                    let column = columns.next();
                    let style = cell.style().unwrap_or_else(|| {
                        column.map_or(ColumnStyle::Default, ColumnFormatRef::style)
                    });
                    style == ColumnStyle::AsciiDoc && contains_index_section(cell.blocks())
                })
            }),
        BlockKind::Admonition(block) => contains_index_section(block.blocks()),
        BlockKind::OrderedList(list) => list
            .items()
            .any(|item| contains_index_section(item.blocks())),
        BlockKind::UnorderedList(list) => list
            .items()
            .any(|item| contains_index_section(item.blocks())),
        BlockKind::DescriptionList(list) => list
            .items()
            .any(|item| contains_index_section(item.blocks())),
        BlockKind::CalloutList(list) => list
            .items()
            .any(|item| contains_index_section(item.blocks())),
        BlockKind::Paragraph(_)
        | BlockKind::DiscreteHeader(_)
        | BlockKind::DocumentAttribute(_)
        | BlockKind::TableOfContents(_)
        | BlockKind::ThematicBreak(_)
        | BlockKind::PageBreak(_)
        | BlockKind::Comment(_)
        | BlockKind::Image(_)
        | BlockKind::Audio(_)
        | BlockKind::Video(_)
        | BlockKind::DelimitedComment(_)
        | BlockKind::Listing(_)
        | BlockKind::Literal(_)
        | BlockKind::Passthrough(_)
        | BlockKind::Verse(_)
        | BlockKind::Stem(_)
        | _ => false,
    })
}

/// Return the rendered level for a section.
///
/// Converters present a source level-zero special section at the chapter tier without changing
/// its document-root placement.
#[must_use]
pub fn effective_section_level(level: u8, kind: SectionKind) -> u8 {
    if level == 0 && kind.is_special() {
        1
    } else {
        level
    }
}

/// Returns the chapter signifier for a numbered level-one book section.
///
/// An explicit empty or unset `chapter-signifier` suppresses the backend's
/// default.
#[must_use]
pub fn book_chapter_signifier<'a>(
    attributes: &'a TraversalContext<'_>,
    default: Option<&'a str>,
) -> Option<&'a str> {
    if !matches!(
        attributes.get("doctype"),
        Some(value) if value.as_str() == Some("book")
    ) {
        return None;
    }

    match attributes
        .get("chapter-signifier")
        .and_then(|value| value.text())
    {
        Some(signifier) if !signifier.is_empty() => Some(signifier),
        Some(_) => None,
        None if attributes.is_explicit("chapter-signifier") => None,
        None => default,
    }
}

/// Format an ordinary section number, with an optional chapter signifier.
#[must_use]
pub fn section_number_prefix(number: &str, signifier: Option<&str>) -> String {
    match signifier {
        Some(signifier) => format!("{signifier} {number}. "),
        None => format!("{number}. "),
    }
}

/// Format a Roman part number, with an optional part signifier.
#[must_use]
pub fn part_number_prefix(number: &str, signifier: Option<&str>) -> String {
    match signifier {
        Some(signifier) => format!("{signifier} {number}: "),
        None => format!("{number}: "),
    }
}

/// Format an appendix letter, with an optional appendix caption.
#[must_use]
pub fn appendix_number_prefix(number: &str, caption: Option<&str>) -> String {
    match caption {
        Some(caption) => format!("{caption} {number}: "),
        None => format!("{number}. "),
    }
}
