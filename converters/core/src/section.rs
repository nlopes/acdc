//! Section presentation utilities shared by converters.

use acdc_parser::{Block, SectionKind};

use crate::TraversalContext;

/// Whether the document holds a section seeded for the generated index.
///
/// The seed is a section the parser classified as [`SectionKind::Index`], which
/// is what `[index]` marks. It is looked for anywhere in the document rather
/// than only as the last top-level section: an index legitimately sits before a
/// bibliography or a colophon, and in a multipart book it can be nested inside
/// a part. Asciidoctor places no ordering constraint on it either.
#[must_use]
pub fn has_index_section(blocks: &[Block<'_>]) -> bool {
    blocks.iter().any(|block| match block {
        Block::Section(section) => {
            section.kind == SectionKind::Index || has_index_section(&section.content)
        }
        // Only a section can be the seed, and only a section nests others.
        Block::TableOfContents(_)
        | Block::Admonition(_)
        | Block::DiscreteHeader(_)
        | Block::DocumentAttribute(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::UnorderedList(_)
        | Block::OrderedList(_)
        | Block::CalloutList(_)
        | Block::DescriptionList(_)
        | Block::DelimitedBlock(_)
        | Block::Paragraph(_)
        | Block::Image(_)
        | Block::Audio(_)
        | Block::Video(_)
        | Block::Comment(_)
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
