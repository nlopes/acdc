//! UI label boundaries and separator keys.

use std::ops::Range;

use crate::{
    ContentAttribute, Error,
    grammar::{ParserState, attributes::AttributeQuote, inline_parser},
};

use super::attributes::parse_content;

fn delimiter_positions<'a>(
    source: &'a str,
    state: &ParserState<'a>,
) -> Result<Vec<(usize, char)>, Error> {
    let mut child = ParserState::for_inline_parsing(source, state, state.inline_ctx);
    inline_parser::ui_separator_positions(source, &mut child).map_err(|_| {
        Error::Parse(
            Box::new(state.create_error_source_location(state.create_location(0, 0))),
            "could not locate UI separators".into(),
        )
    })
}

fn split_ranges(positions: &[(usize, char)], delimiter: char, end: usize) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for &(offset, character) in positions.iter().take_while(|(offset, _)| *offset < end) {
        if character == delimiter {
            ranges.push(start..offset);
            start = offset + character.len_utf8();
        }
    }
    ranges.push(start..end);
    ranges
}

fn parse_fields<'a>(
    state: &mut ParserState<'a>,
    source: &'a str,
    start: usize,
    ranges: Vec<Range<usize>>,
) -> Result<Vec<ContentAttribute<'a>>, Error> {
    ranges
        .into_iter()
        .map(|range| {
            let field = source.get(range.clone()).ok_or_else(|| {
                Error::Parse(
                    Box::new(
                        state.create_error_source_location(state.create_location(start, start)),
                    ),
                    "invalid UI label range".into(),
                )
            })?;
            let from = start + range.start + field.len() - field.trim_start().len();
            parse_content(state, field.trim(), from, AttributeQuote::Unquoted)
        })
        .collect()
}

pub(super) fn parse_keyboard_keys<'a>(
    state: &mut ParserState<'a>,
    source: &'a str,
    start: usize,
) -> Result<Vec<ContentAttribute<'a>>, Error> {
    if source.is_empty() {
        return Ok(Vec::new());
    }
    let trimmed = source.trim();
    let start = start + source.len() - source.trim_start().len();
    let positions = delimiter_positions(trimmed, state)?;
    let delimiter = positions
        .iter()
        .copied()
        .find(|(offset, character)| *offset != 0 && matches!(character, '+' | ','));
    let ranges = if let Some((_, delimiter)) = delimiter {
        let trailing = trimmed.ends_with(delimiter);
        let end = trimmed.len().saturating_sub(usize::from(trailing));
        let mut ranges = split_ranges(&positions, delimiter, end);
        if trailing && let Some(last) = ranges.last_mut() {
            last.end = trimmed.len();
        }
        ranges
    } else {
        std::iter::once(0..trimmed.len()).collect()
    };
    parse_fields(state, trimmed, start, ranges)
}

pub(super) fn parse_menu_items<'a>(
    state: &mut ParserState<'a>,
    source: &'a str,
    start: usize,
) -> Result<Vec<ContentAttribute<'a>>, Error> {
    if source.is_empty() {
        return Ok(Vec::new());
    }
    let positions = delimiter_positions(source, state)?;
    let delimiter = if positions.iter().any(|(_, character)| *character == '>') {
        Some('>')
    } else if positions.iter().any(|(_, character)| *character == ',') {
        Some(',')
    } else {
        None
    };
    let mut ranges = delimiter.map_or_else(
        || std::iter::once(0..source.len()).collect(),
        |delimiter| split_ranges(&positions, delimiter, source.len()),
    );
    while ranges.last().is_some_and(Range::is_empty) {
        ranges.pop();
    }
    parse_fields(state, source, start, ranges)
}
