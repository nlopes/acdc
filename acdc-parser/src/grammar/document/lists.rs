//! List item locations and description-list nesting.

use crate::{
    Block, BlockMetadata, DescriptionList, DescriptionListItem, Location, Title,
    grammar::ParserState,
};
use bumpalo::collections::String as BumpString;
use std::collections::VecDeque;

/// Join a list item's first line and continuation lines with newlines.
/// Return `first_line` unchanged when there are no continuation lines.
/// Otherwise, store the joined text in the arena.
pub(super) fn assemble_principal_text<'a>(
    state: &ParserState<'a>,
    first_line: &'a str,
    continuation_lines: &[&str],
) -> &'a str {
    if continuation_lines.is_empty() {
        first_line
    } else {
        let mut s = BumpString::new_in(state.arena);
        s.push_str(first_line);
        for line in continuation_lines {
            s.push('\n');
            s.push_str(line);
        }
        s.into_bump_str()
    }
}

/// Calculates the end position for a list item based on its principal text.
/// Returns `start` if empty, otherwise one less than `first_line_end`.
pub(super) const fn calculate_item_end(
    principal_text_is_empty: bool,
    start: usize,
    first_line_end: usize,
) -> usize {
    if principal_text_is_empty {
        start
    } else {
        first_line_end.saturating_sub(1)
    }
}

/// Find a description-list marker after at least one term character.
/// A marker must be followed by a space, a newline, or (if `allow_eoi` is set) end of input.
///
/// Stop at the next newline unless `scan_across_eol` is set. In that case, stop
/// at the next blank line. This scan avoids a PEG rule call at each byte.
#[inline]
pub(super) fn find_dlist_marker(
    bytes: &[u8],
    pos: usize,
    scan_across_eol: bool,
    allow_eoi: bool,
) -> bool {
    let mut i = pos;
    while let Some(&b) = bytes.get(i) {
        if b == b'\n' && (!scan_across_eol || bytes.get(i + 1) == Some(&b'\n')) {
            return false;
        }
        if i > pos && (b == b':' || b == b';') {
            let marker_len = if b == b':' {
                let mut k = 1;
                while k < 4 && bytes.get(i + k) == Some(&b':') {
                    k += 1;
                }
                if k >= 2 { k } else { 0 }
            } else if bytes.get(i + 1) == Some(&b';') {
                2
            } else {
                0
            };
            if marker_len > 0 {
                let after = bytes.get(i + marker_len).copied();
                if matches!(after, Some(b'\n' | b' ')) || (allow_eoi && after.is_none()) {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

fn description_list_location(items: &[DescriptionListItem<'_>]) -> Location {
    let Some((first, rest)) = items.split_first() else {
        return Location::default();
    };
    let last = rest.last().unwrap_or(first);
    Location {
        absolute_start: first.location.absolute_start,
        absolute_end: last.location.absolute_end,
        start: first.location.start.clone(),
        end: last.location.end.clone(),
    }
}

fn nest_description_list_items<'input>(
    items: &mut VecDeque<DescriptionListItem<'input>>,
    delimiter: &'input str,
    ancestors: &mut Vec<&'input str>,
) -> Vec<DescriptionListItem<'input>> {
    let mut nested = Vec::new();

    while items
        .front()
        .is_some_and(|item| item.delimiter == delimiter)
    {
        let Some(mut item) = items.pop_front() else {
            break;
        };

        if let Some(child_delimiter) = items.front().map(|child| child.delimiter)
            && child_delimiter != delimiter
            && !ancestors.contains(&child_delimiter)
        {
            // A new delimiter starts one child level; only an ancestor delimiter unwinds it.
            ancestors.push(delimiter);
            let children = nest_description_list_items(items, child_delimiter, ancestors);
            ancestors.pop();
            let location = description_list_location(&children);
            item.location.absolute_end = location.absolute_end;
            item.location.end = location.end.clone();
            item.description
                .push(Block::DescriptionList(DescriptionList {
                    title: Title::default(),
                    metadata: BlockMetadata::default(),
                    items: children,
                    location,
                }));
        }

        nested.push(item);
    }

    nested
}

pub(super) fn build_description_list_topology(
    items: Vec<DescriptionListItem<'_>>,
) -> Vec<DescriptionListItem<'_>> {
    if items
        .first()
        .is_none_or(|first| items.iter().all(|item| item.delimiter == first.delimiter))
    {
        return items;
    }

    let mut items = VecDeque::from(items);
    let delimiter = items.front().map_or("", |item| item.delimiter);
    nest_description_list_items(&mut items, delimiter, &mut Vec::new())
}
