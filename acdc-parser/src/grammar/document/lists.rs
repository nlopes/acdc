//! List item locations and description-list nesting.

use crate::{
    Block, BlockMetadata, DescriptionList, DescriptionListDefinition, DescriptionListItem,
    Location, Title, grammar::ParserState,
};
use bumpalo::collections::String as BumpString;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::grammar) struct OrdinaryListScope<'a> {
    pub(crate) offset: usize,
    pub(crate) unordered: Option<&'a str>,
    pub(crate) ordered: Option<&'a str>,
    /// The markers of every ordinary list open at this offset.
    pub(crate) open: OpenListMarkers,
}

/// A set of ordinary list markers. As in Asciidoctor, a list item continues
/// the open list whose marker it matches, and any other marker starts a
/// nested list.
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::grammar) struct OpenListMarkers {
    /// Bit `n` is set for an open `*` marker of length `n`.
    asterisks: u64,
    hyphen: bool,
    /// Bit `n` is set for an open dotted ordered marker with `n` dots.
    dots: u64,
    numbered: bool,
}

impl OpenListMarkers {
    pub(super) fn insert(&mut self, marker: ListMarker<'_>) {
        match marker {
            ListMarker::Unordered("-") => self.hyphen = true,
            ListMarker::Unordered(marker) => self.asterisks |= length_bit(marker.len()),
            ListMarker::Ordered(marker) if marker.starts_with('.') => {
                self.dots |= length_bit(marker.len());
            }
            ListMarker::Ordered(_) => self.numbered = true,
            ListMarker::Description(_) => {}
        }
    }

    pub(super) fn contains(self, marker: ListMarker<'_>) -> bool {
        match marker {
            ListMarker::Unordered("-") => self.hyphen,
            ListMarker::Unordered(marker) => self.asterisks & length_bit(marker.len()) != 0,
            ListMarker::Ordered(marker) if marker.starts_with('.') => {
                self.dots & length_bit(marker.len()) != 0
            }
            ListMarker::Ordered(_) => self.numbered,
            ListMarker::Description(_) => false,
        }
    }
}

/// The bit for a marker of `length` characters. Markers too long for the set
/// get no bit, so they never match an open list.
fn length_bit(length: usize) -> u64 {
    u32::try_from(length)
        .ok()
        .and_then(|length| 1_u64.checked_shl(length))
        .unwrap_or(0)
}

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::grammar) struct DescriptionListScope<'a> {
    pub(crate) offset: usize,
    // Each of the four delimiter spellings can start one active level.
    markers: [&'a str; 4],
}

impl<'a> DescriptionListScope<'a> {
    pub(crate) fn new(offset: usize) -> Self {
        Self {
            offset,
            ..Self::default()
        }
    }

    pub(crate) fn select(&mut self, marker: &'a str) {
        let mut remaining = self.markers.iter_mut();
        if let Some(current) = remaining.find(|old| old.is_empty() || **old == marker) {
            *current = marker;
            for child in remaining {
                *child = "";
            }
        }
    }

    pub(crate) fn contains(self, marker: &str) -> bool {
        self.markers.contains(&marker)
    }
}

#[derive(Clone, Copy)]
pub(super) enum ListMarker<'a> {
    Unordered(&'a str),
    Ordered(&'a str),
    Description(&'a str),
}

impl ListMarker<'_> {
    /// Whether a description list item with this marker is a sibling or an
    /// ancestor of an item marked `next`.
    pub(super) fn is_sibling_or_ancestor(self, next: Self) -> bool {
        match (self, next) {
            (Self::Description(parent), Self::Description(candidate)) => {
                parent.starts_with(candidate)
            }
            _ => false,
        }
    }

    /// Whether two ordinary list items belong to the same list. Each `*` length,
    /// `-`, and dot count is its own marker; every numbered marker, such as
    /// `1.` or `10.`, matches every other numbered marker.
    pub(super) fn same_ordinary_list(self, other: Self) -> bool {
        let mut open = OpenListMarkers::default();
        open.insert(self);
        open.contains(other) && !matches!(self, Self::Description(_))
    }
}

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

/// Find a description-list marker after at least one term character on this line.
/// A marker must be followed by a space, a newline, or end of input.
/// This scan avoids a PEG rule call at each byte.
#[inline]
pub(super) fn find_dlist_marker(bytes: &[u8], pos: usize) -> bool {
    for (i, &b) in bytes.iter().enumerate().skip(pos) {
        if b == b'\n' {
            return false;
        }
        if i > pos && (b == b':' || b == b';') {
            let marker_len = if b == b':' {
                let colons = bytes
                    .iter()
                    .skip(i)
                    .take(4)
                    .take_while(|&&c| c == b':')
                    .count();
                if colons >= 2 { colons } else { 0 }
            } else if bytes.get(i + 1) == Some(&b';') {
                2
            } else {
                0
            };
            if marker_len > 0 {
                let after = bytes.get(i + marker_len).copied();
                if matches!(after, None | Some(b'\n' | b' ')) {
                    return true;
                }
            }
        }
    }
    false
}

fn item_delimiter<'a>(item: &DescriptionListItem<'a>) -> &'a str {
    item.terms.first().map_or("", |term| term.delimiter)
}

fn group_description_terms(items: Vec<DescriptionListItem<'_>>) -> Vec<DescriptionListItem<'_>> {
    let mut groups: Vec<DescriptionListItem<'_>> = Vec::with_capacity(items.len());
    for item in items {
        if let Some(previous) = groups.last_mut()
            && previous.definition.is_none()
        {
            previous.terms.extend(item.terms);
            previous.definition = item.definition;
            previous.location.absolute_end = item.location.absolute_end;
            previous.location.end = item.location.end;
        } else {
            groups.push(item);
        }
    }
    groups
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
        .is_some_and(|item| item_delimiter(item) == delimiter)
    {
        let Some(mut item) = items.pop_front() else {
            break;
        };

        if let Some(child_delimiter) = items.front().map(item_delimiter)
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
            let definition = item.definition.get_or_insert_with(|| {
                DescriptionListDefinition::new(None, Vec::new(), location.clone())
            });
            definition.location.absolute_end = location.absolute_end;
            definition.location.end = location.end.clone();
            definition
                .blocks
                .push(Block::DescriptionList(DescriptionList {
                    title: Title::default(),
                    metadata: BlockMetadata::default(),
                    items: children,
                    location,
                }));
        }

        nested.push(item);
    }

    group_description_terms(nested)
}

pub(super) fn build_description_list_topology(
    items: Vec<DescriptionListItem<'_>>,
) -> Vec<DescriptionListItem<'_>> {
    if items.first().is_none_or(|first| {
        items
            .iter()
            .all(|item| item_delimiter(item) == item_delimiter(first))
    }) {
        return group_description_terms(items);
    }

    let mut items = VecDeque::from(items);
    let delimiter = items.front().map_or("", item_delimiter);
    nest_description_list_items(&mut items, delimiter, &mut Vec::new())
}
