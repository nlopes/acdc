use acdc_parser::ast::{BlockKind, DocumentRef, ListItemRef, StructuralRef, UnorderedListRef};

use crate::LintId;

use super::{
    LintEmitter, SourceLine, is_list_continuation, is_skipped_line, root_list_family,
    source_line_at,
};

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
                    None,
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
    document: DocumentRef<'_>,
    lines: &[SourceLine<'_>],
) {
    for node in document.descendants() {
        if let StructuralRef::ListItem(item) = node {
            lint_list_item_marker_spacing(emitter, item, lines);
        }
    }
}

fn lint_list_item_marker_spacing(
    emitter: &mut LintEmitter<'_>,
    item: ListItemRef<'_>,
    lines: &[SourceLine<'_>],
) {
    let Some(line) = source_line_at(lines, item.location().start().line()) else {
        return;
    };
    let trimmed = line.text.trim_start();
    let Some(after_marker) = trimmed.strip_prefix(item.marker()) else {
        return;
    };

    if after_marker
        .chars()
        .next()
        .is_some_and(|ch| !ch.is_whitespace())
    {
        let leading_columns = line
            .text
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .count();
        let column = leading_columns
            .saturating_add(item.marker().chars().count())
            .saturating_add(1);
        emitter.emit(
            LintId::ListMarkerSpacing,
            "list marker should be followed by whitespace",
            None,
            Some(emitter.point_location(line.number, column)),
        );
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
                None,
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
                None,
                Some(emitter.point_location(line.number, 1)),
            );
        }
    }
}

pub(crate) fn lint_nested_unordered_list_markers(
    emitter: &mut LintEmitter<'_>,
    document: DocumentRef<'_>,
) {
    lint_nested_markers(emitter, StructuralRef::Document(document), 0);
}

fn lint_nested_markers(emitter: &mut LintEmitter<'_>, node: StructuralRef<'_>, list_depth: usize) {
    if let StructuralRef::Block(block) = node
        && let BlockKind::UnorderedList(list) = block.kind()
    {
        lint_unordered_list(emitter, list, list_depth);
        return;
    }
    let child_depth = if matches!(node, StructuralRef::Block(block) if matches!(
        block.kind(), BlockKind::OrderedList(_) | BlockKind::DescriptionList(_) | BlockKind::CalloutList(_)
    )) {
        list_depth.saturating_add(1)
    } else {
        list_depth
    };
    for child in node.children() {
        lint_nested_markers(emitter, child, child_depth);
    }
}

fn lint_unordered_list(
    emitter: &mut LintEmitter<'_>,
    list: UnorderedListRef<'_>,
    list_depth: usize,
) {
    for item in list.items() {
        if list_depth > 0 && item.marker().trim_start().starts_with('-') {
            emitter.emit(
                LintId::NestedUnorderedListMarker,
                "nested unordered list item uses a hyphen marker",
                None,
                Some(emitter.source_location(item.location())),
            );
        }
        for child in StructuralRef::ListItem(item).children() {
            lint_nested_markers(emitter, child, list_depth.saturating_add(1));
        }
    }
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
    fn list_marker_spacing_allows_valid_list_items() -> Result<(), Error> {
        let report = report_for("= Title\n\n* Item\n- Item\n. Item\n")?;

        assert!(!has_lint(&report, LintId::ListMarkerSpacing));
        Ok(())
    }

    #[test]
    fn list_marker_spacing_ignores_marker_like_paragraphs() -> Result<(), Error> {
        let report = report_for(
            "= Title\n\n*Item\n\n**The grammar mirrors the spec directly**.\n\n*Important* paragraph.\n",
        )?;

        assert!(!has_lint(&report, LintId::ListMarkerSpacing));
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
