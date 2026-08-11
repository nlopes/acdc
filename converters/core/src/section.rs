//! Section presentation utilities shared by converters.

use acdc_parser::{Block, SectionKind};

/// Whether the last section in `blocks` is tagged with the requested style.
///
/// Converters use this to decide whether to defer index-term catalog rendering until the
/// document's explicit index section.
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
