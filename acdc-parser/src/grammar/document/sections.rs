//! Section levels and nesting checks.

use crate::{
    DocumentAttributes, Location, Warning, WarningKind,
    grammar::ParserState,
    model::{LeveloffsetRange, SectionKind, SectionLevel, strip_quotes},
};

/// Add the include-range and document `leveloffset` values to a section level.
/// Keep the result within the supported levels, 0 through 5.
pub(super) fn apply_leveloffset(
    base_level: SectionLevel,
    byte_offset: usize,
    leveloffset_ranges: &[LeveloffsetRange],
    document_attributes: &DocumentAttributes,
) -> SectionLevel {
    let range_offset = crate::model::calculate_leveloffset_at(leveloffset_ranges, byte_offset);

    let attr_offset = document_attributes
        .text("leveloffset")
        .map(strip_quotes)
        .and_then(|s| s.parse::<isize>().ok())
        .unwrap_or(0);

    let total_offset = range_offset + attr_offset;

    if total_offset != 0 {
        let adjusted = isize::from(base_level) + total_offset;

        let clamped = adjusted.clamp(0, 5);

        SectionLevel::try_from(clamped)
            .inspect_err(|_| {
                tracing::error!(clamped, "not a valid section after applying leveloffset");
            })
            .unwrap_or(0)
    } else {
        base_level
    }
}

/// Expected `parent_section_level` (one-based, i.e. own level + 1) for a
/// section's nested content.
///
/// A nestable level-0 special section in a book is rendered at level 1, so its
/// first subsection must be level 2 (`===`). A level-1 (`==`) heading closes the
/// special section instead of nesting under it. Every other section expects
/// children one level deeper than itself.
pub(super) fn expected_child_level(
    level: SectionLevel,
    kind: SectionKind,
    is_book: bool,
) -> SectionLevel {
    if level == 0
        && is_book
        && matches!(
            kind,
            SectionKind::Preface | SectionKind::Abstract | SectionKind::Appendix
        )
    {
        2
    } else {
        level + 1
    }
}

pub(super) fn warn_for_nested_special_section(
    state: &ParserState<'_>,
    direct_parent_section_kind: Option<SectionKind>,
    heading_location: Location,
) {
    let kind = match direct_parent_section_kind {
        Some(SectionKind::Bibliography) => WarningKind::NestedSectionInBibliography,
        Some(SectionKind::Index) => WarningKind::NestedSectionInIndex,
        _ => return,
    };
    let location = state.create_error_source_location(heading_location);
    state.add_warning(Warning::new(kind, Some(location)));
}
