//! List item locations and description-list nesting.

mod callout;
mod construction;
mod description;

use crate::{
    Block, BlockMetadata, CalloutList, CalloutListItem, CalloutMarker, DescriptionList,
    DescriptionListDefinition, DescriptionListItem, DescriptionListTerm, Error, InlineNode,
    ListItem, ListItemCheckedStatus, Location, OrderedList, Title, UnorderedList, Warning,
    WarningKind,
    grammar::{ParserState, helpers::BlockParsingMetadata},
    model::ListLevel,
};
use bumpalo::collections::String as BumpString;
use std::{collections::VecDeque, ops::ControlFlow};

use super::metadata::order_block_prefix_events;

#[derive(Clone, Copy)]
pub(super) enum ListKind<'a> {
    Unordered(OrdinaryListKind<'a>),
    Ordered(OrdinaryListKind<'a>),
    Callout,
    Description,
}

#[derive(Clone, Copy)]
pub(super) struct OrdinaryListKind<'a> {
    pub(super) parent_other: Option<&'a str>,
    pub(super) parent_same: Option<&'a str>,
    pub(super) allow_continuation: bool,
    pub(super) has_own_metadata: bool,
}

pub(super) struct ListEntry<'a> {
    pub(super) start: usize,
    pub(super) body_offset: usize,
    pub(super) metadata: Option<BlockParsingMetadata<'a>>,
    pub(super) kind: ListKind<'a>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ListPhase {
    AfterPrincipal,
    Attachments,
    Sibling,
}

pub(super) type ListStepKind<'a> =
    ControlFlow<Result<super::coordinator::BlockStepKind<'a>, Error>, Option<ListItemHead<'a>>>;

pub(super) struct ListStep<'a> {
    pub(super) kind: ListStepKind<'a>,
    pub(super) end: usize,
}

pub(super) struct DescriptionStep<'a> {
    pub(super) child: Option<Result<super::coordinator::BlockStepKind<'a>, Error>>,
    pub(super) end: usize,
    pub(super) continuing: bool,
    pub(super) warn_on_error: bool,
}

#[derive(Clone, Copy)]
pub(super) struct OrdinaryListLayout {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) offset: usize,
    pub(super) ordered: bool,
    pub(super) has_own_metadata: bool,
}

pub(super) fn build_ordinary_list<'a>(
    state: &ParserState<'a>,
    metadata: &BlockParsingMetadata<'a>,
    layout: OrdinaryListLayout,
    items: Vec<ListItem<'a>>,
) -> Block<'a> {
    let marker = items.first().map_or("", |item| item.marker);
    let title = if layout.has_own_metadata {
        metadata.title.clone()
    } else {
        Title::default()
    };
    let metadata = if layout.has_own_metadata {
        metadata.metadata.clone()
    } else {
        BlockMetadata::default()
    };
    let location = state.create_location(layout.start + layout.offset, layout.end + layout.offset);
    if layout.ordered {
        Block::OrderedList(OrderedList {
            title,
            metadata,
            items,
            marker,
            location,
        })
    } else {
        Block::UnorderedList(UnorderedList {
            title,
            metadata,
            items,
            marker,
            location,
        })
    }
}

pub(super) type CalloutScope = (Option<usize>, Vec<CalloutMarker>);
pub(super) type CalloutItem<'a> = (CalloutListItem<'a>, String, usize);

pub(super) fn build_callout_list<'a>(
    state: &ParserState<'a>,
    block_metadata: &BlockParsingMetadata<'a>,
    location: Location,
    content: Vec<CalloutItem<'a>>,
    callouts: &[CalloutMarker],
) -> Block<'a> {
    let mut auto_number = 0;
    let mut items: Vec<CalloutListItem> = Vec::with_capacity(content.len());

    for (expected_number, (mut item, marker, _end)) in (1..).zip(content) {
        let actual_number = if marker == "<.>" {
            auto_number += 1;
            auto_number.to_string()
        } else {
            marker
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_string()
        };
        if actual_number != expected_number.to_string() {
            state.add_generic_warning_at(
                format!("callout list item index: expected {expected_number}, got {actual_number}"),
                item.location.clone(),
            );
        }
        // List labels use their ordinal even when the source marker is
        // invalid or mixes explicit and automatic numbering.
        item.callout.number = expected_number;

        // Check if the EXPECTED callout exists in the verbatim block
        // (This warns when sequence is broken and the expected number is missing)
        let callout_exists = callouts.iter().any(|c| c.number == expected_number);
        if !callout_exists {
            state.add_generic_warning_at(
                format!("no callout found for <{expected_number}>"),
                item.location.clone(),
            );
        }
        items.push(item);
    }

    Block::CalloutList(CalloutList {
        title: block_metadata.title.clone(),
        metadata: block_metadata.metadata.clone(),
        items,
        location,
    })
}

pub(super) struct CalloutItemHead<'a> {
    pub(super) principal: Result<Vec<InlineNode<'a>>, Error>,
    pub(super) marker: &'a str,
    pub(super) start: usize,
    pub(super) end: usize,
}

impl<'a> CalloutItemHead<'a> {
    pub(super) fn finish(
        self,
        state: &ParserState<'a>,
        blocks: Result<Vec<Block<'a>>, Error>,
        parsed_end: usize,
        offset: usize,
    ) -> Result<CalloutItem<'a>, Error> {
        let principal = self.principal?;
        let blocks = blocks?;
        let location = state.create_location(self.start + offset, self.end + offset);
        let callout = if self.marker == "<.>" {
            CalloutMarker::auto(0, location)
        } else {
            CalloutMarker::explicit(0, location)
        };
        let end = if blocks.is_empty() {
            self.end
        } else {
            parsed_end.saturating_sub(1)
        };
        Ok((
            CalloutListItem {
                callout,
                principal,
                blocks: super::metadata::order_block_prefix_events(blocks),
                location: state.create_location(self.start + offset, end + offset),
            },
            self.marker.to_string(),
            end,
        ))
    }
}

pub(super) type DescriptionPrincipal<'a> = (Vec<InlineNode<'a>>, Vec<InlineNode<'a>>);

pub(super) struct DescriptionTermSource<'a> {
    pub(super) raw: &'a str,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) delimiter_start: usize,
    pub(super) delimiter_end: usize,
    pub(super) delimiter: &'a str,
}

pub(super) struct DescriptionItemSource<'a> {
    pub(super) term: DescriptionTermSource<'a>,
    pub(super) principal_content: &'a str,
    pub(super) principal_start: usize,
}

pub(super) struct DescriptionItemHead<'a> {
    pub(super) source: DescriptionItemSource<'a>,
    pub(super) principal: Result<DescriptionPrincipal<'a>, Error>,
}

pub(super) fn warn_discarded_attachment(state: &ParserState<'_>, error: &Error) {
    state.add_warning(Warning::new(
        WarningKind::ContentRecovery {
            message: format!("discarded attached content: {error}").into(),
        },
        error.source_location().cloned(),
    ));
}

impl<'a> DescriptionItemHead<'a> {
    pub(super) fn finish(
        self,
        state: &ParserState<'a>,
        attached: Vec<Result<Vec<Block<'a>>, Error>>,
        offset: usize,
    ) -> Result<DescriptionListItem<'a>, Error> {
        let principal = self.principal?;
        let mut blocks = Vec::with_capacity(attached.len());
        for content in attached {
            match content {
                Ok(content) => blocks.extend(content),
                Err(error) => warn_discarded_attachment(state, &error),
            }
        }
        Ok(self.source.build(state, principal, blocks, offset))
    }
}

impl<'a> DescriptionItemSource<'a> {
    pub(super) fn build(
        self,
        state: &ParserState<'a>,
        principal: DescriptionPrincipal<'a>,
        description: Vec<Block<'a>>,
        offset: usize,
    ) -> DescriptionListItem<'a> {
        let Self {
            term:
                DescriptionTermSource {
                    start: term_start,
                    delimiter_start: delim_start,
                    delimiter_end: delim_end,
                    delimiter,
                    ..
                },
            principal_content,
            principal_start,
        } = self;
        let (term, principal_text) = principal;
        let principal_end = principal_start + principal_content.trim_end().len();
        let actual_end = description.last().map_or_else(
            || {
                state
                    .create_block_location(term_start, principal_end.max(delim_end), offset)
                    .absolute_end
                    - offset
            },
            |block| block.location().absolute_end - offset,
        );

        let delimiter_location = state.create_block_location(delim_start, delim_end, offset);
        let term = DescriptionListTerm {
            anchors: Vec::new(),
            content: term,
            delimiter,
            delimiter_location: Some(delimiter_location),
            location: state.create_block_location(term_start, delim_end, offset),
        };
        let principal = (!principal_content.trim().is_empty()).then_some(principal_text);
        let blocks = order_block_prefix_events(description);
        let definition = if principal.is_some() || !blocks.is_empty() {
            let location = if principal.is_some() {
                let start = principal_start + principal_content.len()
                    - principal_content.trim_start().len();
                state.create_location(start + offset, actual_end + offset)
            } else {
                let mut location = blocks
                    .first()
                    .map_or_else(Location::default, |block| block.location().clone());
                if let Some(last) = blocks.last() {
                    location.absolute_end = last.location().absolute_end;
                    location.end = last.location().end.clone();
                }
                location
            };
            Some(DescriptionListDefinition {
                principal,
                blocks,
                location,
            })
        } else {
            None
        };
        let mut location = state.create_location(term_start + offset, actual_end + offset);
        if definition.is_none() {
            location.absolute_end = term.location.absolute_end;
            location.end = term.location.end.clone();
        }
        DescriptionListItem {
            terms: vec![term],
            definition,
            location,
        }
    }
}

pub(super) fn build_description_list<'a>(
    metadata: &BlockParsingMetadata<'a>,
    location: Location,
    items: Vec<DescriptionListItem<'a>>,
) -> Block<'a> {
    Block::DescriptionList(DescriptionList {
        title: metadata.title.clone(),
        metadata: metadata.metadata.clone(),
        items: build_description_list_topology(items),
        location,
    })
}

/// An accepted principal, before its nested and attached blocks are built.
pub(super) struct ListItemHead<'a> {
    pub(super) principal: Result<Vec<InlineNode<'a>>, Error>,
    pub(super) marker: &'a str,
    pub(super) checked: Option<ListItemCheckedStatus>,
    pub(super) start: usize,
    pub(super) end: usize,
}

impl<'a> ListItemHead<'a> {
    pub(super) fn finish(
        self,
        state: &ParserState<'a>,
        blocks: Vec<Block<'a>>,
        parsed_end: usize,
        offset: usize,
    ) -> Result<(ListItem<'a>, usize), Error> {
        let level =
            ListLevel::try_from(ListItem::parse_depth_from_marker(self.marker).unwrap_or(1))?;
        let principal = self.principal?;
        let end = if blocks.is_empty() {
            self.end
        } else {
            parsed_end.saturating_sub(1)
        };
        Ok((
            ListItem {
                principal,
                blocks: super::metadata::order_block_prefix_events(blocks),
                level,
                marker: self.marker,
                checked: self.checked,
                location: state.create_location(self.start + offset, end + offset),
            },
            end,
        ))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::grammar) struct OrdinaryListScope<'a> {
    pub(crate) offset: usize,
    pub(crate) unordered: Option<&'a str>,
    pub(crate) ordered: Option<&'a str>,
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
    pub(super) fn is_sibling_or_ancestor(self, next: Self) -> bool {
        match (self, next) {
            (Self::Unordered(parent), Self::Unordered(candidate))
            | (Self::Description(parent), Self::Description(candidate)) => {
                parent.starts_with(candidate)
            }
            (Self::Ordered(parent), Self::Ordered(candidate)) => {
                parent.starts_with('.') == candidate.starts_with('.')
                    && candidate.bytes().filter(|byte| *byte == b'.').count()
                        <= parent.bytes().filter(|byte| *byte == b'.').count()
            }
            _ => false,
        }
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
    let mut i = pos;
    while let Some(&b) = bytes.get(i) {
        if b == b'\n' {
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
                if matches!(after, None | Some(b'\n' | b' ')) {
                    return true;
                }
            }
        }
        i += 1;
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
