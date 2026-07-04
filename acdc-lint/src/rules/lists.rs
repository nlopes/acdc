use acdc_parser::{Block, DelimitedBlock, DelimitedBlockType, UnorderedList};

use crate::LintId;

use super::{LintEmitter, SourceLine, is_list_continuation, is_skipped_line, root_list_family};

pub(crate) fn lint_adjacent_list_separator(
    emitter: &mut LintEmitter<'_>,
    lines: &[SourceLine<'_>],
    skipped_lines: &[bool],
) {
    let mut last_list_family = None;
    let mut blank_since_last_list = false;
    let mut comment_separator_since_last_list = false;

    for line in lines {
        if is_skipped_line(line.number, skipped_lines) {
            last_list_family = None;
            blank_since_last_list = false;
            comment_separator_since_last_list = false;
            continue;
        }

        let trimmed = line.text.trim();
        if trimmed.is_empty() {
            blank_since_last_list = last_list_family.is_some();
            continue;
        }
        if trimmed.starts_with("//") {
            if blank_since_last_list {
                comment_separator_since_last_list = true;
            }
            continue;
        }

        if let Some(family) = root_list_family(line.text) {
            if last_list_family == Some(family)
                && blank_since_last_list
                && !comment_separator_since_last_list
            {
                emitter.emit(
                    LintId::AdjacentListSeparator,
                    "adjacent lists should be separated with an empty line comment",
                    Some("insert a line comment such as `//-` between the lists".to_string()),
                    Some(emitter.point_location(line.number, 1)),
                );
            }
            last_list_family = Some(family);
            blank_since_last_list = false;
            comment_separator_since_last_list = false;
        } else if !is_list_continuation(trimmed) {
            last_list_family = None;
            blank_since_last_list = false;
            comment_separator_since_last_list = false;
        }
    }
}

pub(crate) fn lint_list_marker_spacing(
    emitter: &mut LintEmitter<'_>,
    lines: &[SourceLine<'_>],
    skipped_lines: &[bool],
) {
    for line in lines {
        if is_skipped_line(line.number, skipped_lines) {
            continue;
        }
        let trimmed = line.text.trim_start();
        if let Some(marker_len) = bad_list_marker_len(trimmed) {
            let column = line
                .text
                .len()
                .saturating_sub(trimmed.len())
                .saturating_add(marker_len)
                .saturating_add(1);
            emitter.emit(
                LintId::ListMarkerSpacing,
                "list marker should be followed by whitespace",
                Some("insert a space after the list marker".to_string()),
                Some(emitter.point_location(line.number, column)),
            );
        }
    }
}

pub(crate) fn lint_ordered_list_explicit_numbers(
    emitter: &mut LintEmitter<'_>,
    lines: &[SourceLine<'_>],
    skipped_lines: &[bool],
) {
    for line in lines {
        if is_skipped_line(line.number, skipped_lines) {
            continue;
        }
        let trimmed = line.text.trim_start();
        if explicit_ordered_marker(trimmed).is_some() {
            emitter.emit(
                LintId::OrderedListExplicitNumber,
                "ordered list item uses an explicit number",
                Some("use AsciiDoc dot syntax such as `. item`".to_string()),
                Some(emitter.point_location(line.number, 1)),
            );
        }
    }
}

pub(crate) fn lint_description_list_bold_terms(
    emitter: &mut LintEmitter<'_>,
    lines: &[SourceLine<'_>],
    skipped_lines: &[bool],
) {
    for line in lines {
        if is_skipped_line(line.number, skipped_lines) {
            continue;
        }
        let trimmed = line.text.trim_start();
        if bold_description_term(trimmed).is_some() {
            emitter.emit(
                LintId::DescriptionListBoldTerm,
                "bold term paragraph used where a description list fits",
                Some("use description-list syntax such as `Term:: description`".to_string()),
                Some(emitter.point_location(line.number, 1)),
            );
        }
    }
}

pub(crate) fn lint_nested_unordered_list_markers(
    emitter: &mut LintEmitter<'_>,
    blocks: &[Block<'_>],
    list_depth: usize,
) {
    for block in blocks {
        match block {
            Block::Admonition(block) => {
                lint_nested_unordered_list_markers(emitter, &block.blocks, list_depth);
            }
            Block::CalloutList(list) => {
                for item in &list.items {
                    lint_nested_unordered_list_markers(
                        emitter,
                        &item.blocks,
                        list_depth.saturating_add(1),
                    );
                }
            }
            Block::DescriptionList(list) => {
                for item in &list.items {
                    lint_nested_unordered_list_markers(
                        emitter,
                        &item.description,
                        list_depth.saturating_add(1),
                    );
                }
            }
            Block::DelimitedBlock(block) => {
                lint_nested_unordered_list_markers_delimited_block(emitter, block, list_depth);
            }
            Block::OrderedList(list) => {
                for item in &list.items {
                    lint_nested_unordered_list_markers(
                        emitter,
                        &item.blocks,
                        list_depth.saturating_add(1),
                    );
                }
            }
            Block::Section(section) => {
                lint_nested_unordered_list_markers(emitter, &section.content, list_depth);
            }
            Block::UnorderedList(list) => lint_unordered_list(emitter, list, list_depth),
            Block::Audio(_)
            | Block::Comment(_)
            | Block::DiscreteHeader(_)
            | Block::DocumentAttribute(_)
            | Block::Image(_)
            | Block::PageBreak(_)
            | Block::Paragraph(_)
            | Block::TableOfContents(_)
            | Block::ThematicBreak(_)
            | Block::Video(_)
            | _ => {}
        }
    }
}

fn lint_nested_unordered_list_markers_delimited_block(
    emitter: &mut LintEmitter<'_>,
    block: &DelimitedBlock<'_>,
    list_depth: usize,
) {
    match &block.inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks) => {
            lint_nested_unordered_list_markers(emitter, blocks, list_depth);
        }
        DelimitedBlockType::DelimitedTable(table) => {
            for row in table
                .header
                .iter()
                .chain(table.rows.iter())
                .chain(table.footer.iter())
            {
                for column in &row.columns {
                    lint_nested_unordered_list_markers(emitter, &column.content, list_depth);
                }
            }
        }
        DelimitedBlockType::DelimitedComment(_)
        | DelimitedBlockType::DelimitedListing(_)
        | DelimitedBlockType::DelimitedLiteral(_)
        | DelimitedBlockType::DelimitedPass(_)
        | DelimitedBlockType::DelimitedStem(_)
        | DelimitedBlockType::DelimitedVerse(_)
        | _ => {}
    }
}

fn lint_unordered_list(emitter: &mut LintEmitter<'_>, list: &UnorderedList<'_>, list_depth: usize) {
    for item in &list.items {
        if list_depth > 0 && item.marker.trim_start().starts_with('-') {
            emitter.emit(
                LintId::NestedUnorderedListMarker,
                "nested unordered list item uses a hyphen marker",
                Some("use asterisk markers for nested unordered lists".to_string()),
                Some(emitter.source_location(&item.location)),
            );
        }
        lint_nested_unordered_list_markers(emitter, &item.blocks, list_depth.saturating_add(1));
    }
}

fn bad_list_marker_len(trimmed: &str) -> Option<usize> {
    if bold_description_term(trimmed).is_some() {
        return None;
    }

    if let Some(rest) = trimmed.strip_prefix('-')
        && rest.chars().next().is_some_and(|ch| !ch.is_whitespace())
    {
        return Some(1);
    }

    let marker_len = trimmed.chars().take_while(|ch| *ch == '*').count();
    if marker_len > 0
        && trimmed
            .chars()
            .nth(marker_len)
            .is_some_and(|ch| !ch.is_whitespace())
    {
        return Some(marker_len);
    }

    let dot_len = trimmed.chars().take_while(|ch| *ch == '.').count();
    if dot_len > 1
        && trimmed
            .chars()
            .nth(dot_len)
            .is_some_and(|ch| !ch.is_whitespace())
    {
        return Some(dot_len);
    }

    let marker_len = explicit_ordered_marker(trimmed)?;
    trimmed
        .chars()
        .nth(marker_len)
        .is_some_and(|ch| !ch.is_whitespace())
        .then_some(marker_len)
}

fn bold_description_term(trimmed: &str) -> Option<usize> {
    bold_description_term_with_marker(trimmed, "**")
        .or_else(|| bold_description_term_with_marker(trimmed, "*"))
}

fn bold_description_term_with_marker(trimmed: &str, marker: &str) -> Option<usize> {
    let rest = trimmed.strip_prefix(marker)?;
    let close = format!("{marker}:");
    let end = rest.find(&close)?;
    if end == 0 {
        return None;
    }
    let after = rest.get(end.saturating_add(close.len())..)?;
    (after.is_empty() || after.starts_with(char::is_whitespace)).then_some(end + marker.len())
}

fn explicit_ordered_marker(trimmed: &str) -> Option<usize> {
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || trimmed.chars().nth(digits) != Some('.') {
        return None;
    }
    trimmed
        .chars()
        .nth(digits.saturating_add(1))
        .is_some()
        .then_some(digits.saturating_add(1))
}

#[cfg(test)]
mod tests {
    use crate::{Error, LintId};

    use super::super::test_support::{has_lint, report_for};

    #[test]
    fn nested_unordered_list_marker_flags_nested_hyphen() -> Result<(), Error> {
        let report = report_for("= Title\n\n* Parent\n+\n- Child\n")?;

        assert!(has_lint(&report, LintId::NestedUnorderedListMarker));
        Ok(())
    }

    #[test]
    fn adjacent_list_separator_flags_same_family_lists() -> Result<(), Error> {
        let report = report_for("= Title\n\n* First\n\n* Second\n")?;

        assert!(has_lint(&report, LintId::AdjacentListSeparator));
        Ok(())
    }

    #[test]
    fn list_marker_spacing_flags_missing_space() -> Result<(), Error> {
        let report = report_for("= Title\n\n*Item\n")?;

        assert!(has_lint(&report, LintId::ListMarkerSpacing));
        Ok(())
    }

    #[test]
    fn explicit_ordered_numbers_are_flagged() -> Result<(), Error> {
        let report = report_for("= Title\n\n1. First\n")?;

        assert!(has_lint(&report, LintId::OrderedListExplicitNumber));
        Ok(())
    }

    #[test]
    fn bold_description_terms_are_flagged() -> Result<(), Error> {
        let report = report_for("= Title\n\n*Term*: description\n")?;

        assert!(has_lint(&report, LintId::DescriptionListBoldTerm));
        assert!(!has_lint(&report, LintId::ListMarkerSpacing));
        Ok(())
    }
}
