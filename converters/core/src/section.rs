//! Section presentation utilities shared by converters.

use acdc_parser::{Block, SectionKind};

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

/// Whether the section hierarchy contains an index section.
///
/// Includes sections nested in book parts. Other block containers and nested
/// table-cell documents are outside this search.
#[must_use]
pub fn has_index_section(blocks: &[Block<'_>]) -> bool {
    blocks.iter().any(|block| {
        if let Block::Section(section) = block {
            section.kind == SectionKind::Index || has_index_section(&section.content)
        } else {
            false
        }
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
