//! UI label boundaries and separator keys.

use std::ops::Range;

use crate::{
    ContentAttribute, Error, Substitution,
    grammar::{ParserState, attributes::AttributeQuote, inline_parser},
};

use super::{
    attributes::parse_content,
    index_terms::IndexTermSegment,
    text::{registration_source, restored_label_offset},
};

pub(super) struct Separator {
    pub(super) range: Range<usize>,
    pub(super) character: char,
}

pub(super) fn menu_character_separator_enabled(state: &ParserState<'_>) -> bool {
    state
        .inline_ctx
        .substitutions
        .precedes(&Substitution::SpecialChars, &Substitution::Macros)
}

pub(super) fn menu_entity_separator_enabled(state: &ParserState<'_>) -> bool {
    // Replacements restore authored entities after special-character escaping.
    let plan = state.inline_ctx.substitutions;
    !menu_character_separator_enabled(state)
        || (plan.precedes(&Substitution::SpecialChars, &Substitution::Replacements)
            && plan.precedes(&Substitution::Replacements, &Substitution::Macros))
}

fn separators<'a>(
    source: &'a str,
    start: usize,
    state: &ParserState<'a>,
) -> Result<Vec<Separator>, Error> {
    let (source, ranges) = registration_source(
        state,
        IndexTermSegment {
            text: source,
            start,
        },
    );
    let mut child = ParserState::for_inline_parsing(source, state, state.inline_ctx);
    inline_parser::ui_separators(source, &mut child)
        .map(|separators| {
            separators
                .into_iter()
                .map(|mut separator| {
                    let start = restored_label_offset(separator.range.start, &ranges, false);
                    let end =
                        restored_label_offset(separator.range.end.saturating_sub(1), &ranges, true)
                            + 1;
                    separator.range = start..end;
                    separator
                })
                .collect()
        })
        .map_err(|_| {
            Error::Parse(
                Box::new(state.create_error_source_location(state.create_location(start, start))),
                "could not locate UI separators".into(),
            )
        })
}

fn split_ranges(separators: &[Separator], delimiter: char, end: usize) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for separator in separators
        .iter()
        .take_while(|separator| separator.range.start < end)
    {
        if separator.character == delimiter {
            ranges.push(start..separator.range.start);
            start = separator.range.end;
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
    let separators = separators(trimmed, start, state)?;
    let delimiter = separators
        .iter()
        .find(|separator| separator.range.start != 0 && matches!(separator.character, '+' | ','));
    let ranges = if let Some(separator) = delimiter {
        let delimiter = separator.character;
        let trailing = trimmed.ends_with(delimiter);
        let end = trimmed.len().saturating_sub(usize::from(trailing));
        let mut ranges = split_ranges(&separators, delimiter, end);
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
    let separators = separators(source, start, state)?;
    let delimiter = if separators
        .iter()
        .any(|separator| separator.character == '>')
    {
        Some('>')
    } else if separators
        .iter()
        .any(|separator| separator.character == ',')
    {
        Some(',')
    } else {
        None
    };
    let mut ranges = delimiter.map_or_else(
        || std::iter::once(0..source.len()).collect(),
        |delimiter| split_ranges(&separators, delimiter, source.len()),
    );
    while ranges.last().is_some_and(Range::is_empty) {
        ranges.pop();
    }
    parse_fields(state, source, start, ranges)
}
