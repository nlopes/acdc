//! Section presentation utilities shared by converters.

use acdc_parser::{Block, ColumnStyle, DelimitedBlockType, SectionKind};

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
    blocks.iter().any(|block| match block {
        Block::Section(section) => {
            section.kind == SectionKind::Index || has_index_section(&section.content)
        }
        Block::DelimitedBlock(block) => match &block.inner {
            DelimitedBlockType::DelimitedExample(blocks)
            | DelimitedBlockType::DelimitedOpen(blocks)
            | DelimitedBlockType::DelimitedQuote(blocks)
            | DelimitedBlockType::DelimitedSidebar(blocks) => has_index_section(blocks),
            DelimitedBlockType::DelimitedTable(table) => table
                .header
                .iter()
                .chain(&table.rows)
                .chain(&table.footer)
                .any(|row| {
                    row.columns.iter().enumerate().any(|(index, cell)| {
                        let style = cell.style.unwrap_or_else(|| {
                            table
                                .columns
                                .get(index)
                                .map_or(ColumnStyle::Default, |column| column.style)
                        });
                        style == ColumnStyle::AsciiDoc && has_index_section(&cell.content)
                    })
                }),
            DelimitedBlockType::DelimitedComment(_)
            | DelimitedBlockType::DelimitedListing(_)
            | DelimitedBlockType::DelimitedLiteral(_)
            | DelimitedBlockType::DelimitedPass(_)
            | DelimitedBlockType::DelimitedVerse(_)
            | DelimitedBlockType::DelimitedStem(_)
            | _ => false,
        },
        Block::Admonition(block) => has_index_section(&block.blocks),
        Block::OrderedList(list) => list
            .items
            .iter()
            .any(|item| has_index_section(&item.blocks)),
        Block::UnorderedList(list) => list
            .items
            .iter()
            .any(|item| has_index_section(&item.blocks)),
        Block::DescriptionList(list) => list
            .items
            .iter()
            .any(|item| has_index_section(&item.description)),
        Block::CalloutList(list) => list
            .items
            .iter()
            .any(|item| has_index_section(&item.blocks)),
        Block::Paragraph(_)
        | Block::DiscreteHeader(_)
        | Block::DocumentAttribute(_)
        | Block::TableOfContents(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::Comment(_)
        | Block::Image(_)
        | Block::Audio(_)
        | Block::Video(_)
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
