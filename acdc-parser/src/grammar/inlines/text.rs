//! Inline label processing and source locations.

use crate::{
    InlineNode, Substitution,
    grammar::{
        ParserState,
        helpers::BlockParsingMetadata,
        inline_preprocessor::SourceMap,
        inline_processing::{process_inlines, process_inlines_no_autolinks},
        inlines::index_terms::IndexTermSegment,
    },
};
use std::ops::Range;

pub(super) fn process_unescaped_xref_label<'a>(
    state: &mut ParserState<'a>,
    metadata: &BlockParsingMetadata<'_>,
    text: &'a str,
    start: usize,
    source_map: &SourceMap,
) -> Result<Vec<InlineNode<'a>>, crate::Error> {
    // Use the unescaped input for UTF-8 boundaries until all locations are mapped.
    let mut inline_ctx = state.inline_ctx;
    inline_ctx.offset = 0;
    let mut child = ParserState::for_inline_parsing(text, state, inline_ctx);
    let end = start + source_map.map_position(text.len())?;
    // Each recorded quote escape removes one byte from the original label.
    let unescaped_offset = |position| {
        position
            - source_map
                .replacements
                .partition_point(|replacement| replacement.absolute_start < position)
    };
    child.attribute_value_ranges = state
        .attribute_value_ranges
        .iter()
        .filter_map(|range| {
            let range_start = range.start.max(start);
            let range_end = range.end.min(end);
            (range_start < range_end)
                .then(|| unescaped_offset(range_start - start)..unescaped_offset(range_end - start))
        })
        .collect();
    let mut inlines = process_inlines_no_autolinks(&mut child, metadata, 0, text.len(), 0, text)?;
    let mut error = None;
    for inline in &mut inlines {
        crate::grammar::location_walk::walk_inline_locations_mut(inline, &mut |location| {
            match source_map
                .map_position(location.absolute_start)
                .and_then(|mapped_start| {
                    // A single-character inline can extend past the label's last byte.
                    source_map
                        .map_end_position(location.absolute_end.min(text.len() - 1))
                        .map(|mapped_end| (mapped_start, mapped_end))
                }) {
                Ok((mapped_start, mapped_end)) => {
                    *location = state.create_location(start + mapped_start, start + mapped_end);
                }
                Err(cause) => error = Some(cause),
            }
        });
    }
    error.map_or(Ok(inlines), Err)
}

pub(super) fn map_registered_inline(
    node: &mut InlineNode<'_>,
    state: &ParserState<'_>,
    start: usize,
    restored_ranges: &[RestoredRange],
) {
    crate::grammar::location_walk::walk_inline_locations_mut(node, &mut |location| {
        location.absolute_start =
            start + restored_label_offset(location.absolute_start, restored_ranges, false);
        location.absolute_end =
            start + restored_label_offset(location.absolute_end, restored_ranges, true);
        location.start = state
            .line_map
            .offset_to_position(location.absolute_start, state.input);
        location.end = state
            .line_map
            .offset_to_position(location.absolute_end, state.input);
    });
}

pub(super) fn parse_footnote_content<'a>(
    state: &mut ParserState<'a>,
    content: IndexTermSegment<'a>,
) -> Result<Vec<InlineNode<'a>>, &'static str> {
    let plan = state.inline_ctx.substitutions;
    let frozen = plan.precedes(&Substitution::Macros, &Substitution::Attributes)
        || plan.precedes(&Substitution::Macros, &Substitution::Quotes)
        || plan.precedes(&Substitution::Macros, &Substitution::Replacements);
    if !frozen {
        let metadata = BlockParsingMetadata {
            substitutions: plan,
            ..BlockParsingMetadata::default()
        };
        return process_inlines(
            state,
            &metadata,
            content.start,
            content.start + content.text.len(),
            state.inline_ctx.offset,
            content.text,
        )
        .map(|(nodes, _)| nodes)
        .map_err(|_| "could not process footnote content");
    }
    let (source, ranges) = registration_source(state, content);
    let mut inline_ctx = state.inline_ctx;
    inline_ctx.offset = 0;
    inline_ctx.substitutions = plan.through(&Substitution::Macros);
    let mut child = ParserState::for_inline_parsing(source, state, inline_ctx);
    let metadata = BlockParsingMetadata {
        substitutions: inline_ctx.substitutions,
        ..BlockParsingMetadata::default()
    };
    let (mut nodes, _) = process_inlines(&mut child, &metadata, 0, source.len(), 0, source)
        .map_err(|_| "could not process footnote content")?;
    for node in &mut nodes {
        map_registered_inline(node, state, content.start, &ranges);
    }
    Ok(nodes)
}

pub(super) type RestoredRange = (Range<usize>, Range<usize>);

pub(super) fn registration_source<'a>(
    state: &ParserState<'a>,
    content: IndexTermSegment<'a>,
) -> (&'a str, Vec<RestoredRange>) {
    if !state
        .inline_ctx
        .substitutions
        .precedes(&Substitution::Macros, &Substitution::Attributes)
    {
        return (content.text, Vec::new());
    }
    let end = content.start + content.text.len();
    let mut source = String::new();
    let mut cursor = content.start;
    let mut restored_ranges = Vec::new();
    for (range, reference) in &state.late_attribute_sources {
        if range.start < cursor || range.end > end {
            continue;
        }
        source.push_str(&state.input[cursor..range.start]);
        let restored_start = source.len();
        source.push_str(reference);
        restored_ranges.push((
            restored_start..source.len(),
            range.start - content.start..range.end - content.start,
        ));
        cursor = range.end;
    }
    if restored_ranges.is_empty() {
        return (content.text, restored_ranges);
    }
    source.push_str(&state.input[cursor..end]);
    (state.intern_str(&source), restored_ranges)
}

pub(super) fn restored_label_offset(offset: usize, ranges: &[RestoredRange], end: bool) -> usize {
    let Some((restored, original)) = ranges.iter().rev().find(|(range, _)| offset >= range.start)
    else {
        return offset;
    };
    if offset >= restored.end {
        original.end + offset - restored.end
    } else if end {
        original.end.saturating_sub(1).max(original.start)
    } else {
        original.start
    }
}

// Log only the static rule message, which does not contain document text.
macro_rules! process_inlines_or_err {
    ($call:expr, $msg:literal) => {
        $call.map_err(|_| {
            tracing::error!($msg);
            $msg
        })
    };
}

pub(super) use process_inlines_or_err;
