use std::ops::Range;

use crate::{
    InlineNode, Location, Raw, Substitution, Symbol,
    grammar::{
        ParserState,
        inlines::typography::{
            Edges, ReplacementValue, WordContext, arrow_allowed, is_arrow, replacements,
        },
        utf8_utils::{RoundDirection, step_char},
    },
    model::substitution::SubstitutionPlan,
};

use super::fragment_offset;

type Fragment<'raw, 'input> = (Range<usize>, &'raw Raw<'input>);

pub(super) fn finish_replacement_stage<'a>(
    fragments: Vec<Raw<'a>>,
    state: &ParserState<'a>,
    edges: Edges,
) -> Vec<InlineNode<'a>> {
    let (prefix, suffix, length) = if let [raw] = fragments.as_slice() {
        let (prefix, suffix) = super::super::inlines::typography::pending_word_edges(raw.content);
        (prefix, suffix, raw.content.len())
    } else {
        let text: String = fragments.iter().map(|raw| raw.content).collect();
        let (prefix, suffix) = super::super::inlines::typography::pending_word_edges(&text);
        (prefix, suffix, text.len())
    };
    let mut leading = Vec::new();
    let mut core = Vec::new();
    let mut trailing = Vec::new();
    let mut offset = 0;
    let end = length - suffix;
    for raw in fragments {
        let start = offset;
        offset += raw.content.len();
        if offset <= prefix {
            leading.push(raw);
        } else if start >= end {
            trailing.push(raw);
        } else if start >= prefix && offset <= end {
            core.push(raw);
        } else {
            for (range, destination) in [
                (0..prefix, &mut leading),
                (prefix..end, &mut core),
                (end..length, &mut trailing),
            ] {
                let begin = start.max(range.start);
                let stop = offset.min(range.end);
                if begin < stop {
                    destination.push(Raw {
                        content: &raw.content[begin - start..stop - start],
                        location: super::raw_segment_location(
                            &raw,
                            begin - start,
                            stop - start,
                            state,
                        ),
                        subs: raw.subs.clone(),
                    });
                }
            }
        }
    }
    let mut resolved = resolve_fragments(
        core,
        state,
        Edges {
            start: edges.start && prefix == 0,
            end: edges.end && suffix == 0,
        },
        false,
    );
    for node in &mut resolved {
        if let InlineNode::RawText(raw) = node {
            raw.subs
                .retain(|substitution| *substitution != Substitution::Replacements);
        }
    }
    leading
        .into_iter()
        .map(InlineNode::RawText)
        .chain(resolved)
        .chain(trailing.into_iter().map(InlineNode::RawText))
        .collect()
}

pub(super) fn resolve_fragments<'a>(
    fragments: Vec<Raw<'a>>,
    state: &ParserState<'a>,
    edges: Edges,
    late_attributes: bool,
) -> Vec<InlineNode<'a>> {
    if !fragments.iter().any(|raw| {
        raw.subs.contains(&Substitution::Replacements)
            && raw
                .content
                .bytes()
                .any(|byte| matches!(byte, b'(' | b'.' | b'\'' | b'-' | b'=' | b'<'))
    }) {
        return fragments.into_iter().map(InlineNode::RawText).collect();
    }
    let text: String = fragments.iter().map(|raw| raw.content).collect();
    let mut offset = 0;
    let spans = fragments
        .iter()
        .map(|raw| {
            let start = offset;
            offset += raw.content.len();
            (start..offset, raw)
        })
        .collect::<Vec<_>>();
    let edits = replacements(&text, edges, WordContext::default(), |range, kind| {
        let first = spans.partition_point(|(span, _)| span.end <= range.start);
        let end = spans.partition_point(|(span, _)| span.start < range.end);
        spans.get(first..end).is_some_and(|covered| {
            !covered.is_empty()
                && covered.iter().all(|(span, raw)| {
                    raw.subs.contains(&Substitution::Replacements)
                        && (!is_arrow(kind)
                            || text[range.clone()].contains('&')
                                && !text[range.start.max(span.start)..range.end.min(span.end)]
                                    .contains('&')
                            || arrow_allowed(
                                &text,
                                range.clone(),
                                kind,
                                SubstitutionPlan::from_substitutions(&raw.subs),
                            ))
                })
                && (!late_attributes
                    || !covered.windows(2).any(|pair| {
                        let [(_, left), (right_span, right)] = pair else {
                            return false;
                        };
                        range.start < right_span.start
                            && right_span.start < range.end
                            && left.location.absolute_end + 1 < right.location.absolute_start
                    }))
        })
    });
    if edits.is_empty() {
        return fragments.into_iter().map(InlineNode::RawText).collect();
    }
    let text = state.intern_str(&text);
    let mut result = Vec::new();
    let mut cursor = 0;
    for edit in edits {
        append_fragments(
            &mut result,
            text,
            &spans,
            cursor..edit.range.start,
            None,
            state,
        );
        match edit.value {
            ReplacementValue::Remove => {}
            ReplacementValue::Symbol(kind) => result.push(InlineNode::Symbol(Symbol {
                kind,
                location: locate(text, &spans, edit.range.clone(), state),
            })),
            ReplacementValue::EncodedLiteral { range, decoded } => append_encoded_literal(
                &mut result,
                text,
                &spans,
                range,
                edit.range.clone(),
                decoded,
                state,
            ),
            ReplacementValue::Literal(range) => append_fragments(
                &mut result,
                text,
                &spans,
                range,
                Some(edit.range.start),
                state,
            ),
        }
        cursor = edit.range.end;
    }
    append_fragments(&mut result, text, &spans, cursor..text.len(), None, state);
    result
}

fn append_encoded_literal<'a>(
    result: &mut Vec<InlineNode<'a>>,
    text: &'a str,
    fragments: &[Fragment<'_, 'a>],
    range: Range<usize>,
    original: Range<usize>,
    decoded: &'static str,
    state: &ParserState<'a>,
) {
    let first = fragments.partition_point(|(span, _)| span.end <= range.start);
    if fragments
        .get(first)
        .is_some_and(|(_, raw)| raw.subs.contains(&Substitution::SpecialChars))
    {
        append_fragments(result, text, fragments, range, Some(original.start), state);
    } else {
        result.push(InlineNode::RawText(Raw {
            content: decoded,
            location: locate(text, fragments, original, state),
            subs: vec![Substitution::SpecialChars],
        }));
    }
}

fn append_fragments<'a>(
    result: &mut Vec<InlineNode<'a>>,
    text: &'a str,
    fragments: &[Fragment<'_, 'a>],
    range: Range<usize>,
    literal_start: Option<usize>,
    state: &ParserState<'a>,
) {
    if range.is_empty() {
        return;
    }
    let begin = fragments.partition_point(|(span, _)| span.end <= range.start);
    let mut first = true;
    for (span, raw) in fragments
        .get(begin..)
        .unwrap_or_default()
        .iter()
        .take_while(|(span, _)| span.start < range.end)
    {
        let left = range.start.max(span.start);
        let right = range.end.min(span.end);
        if left >= right {
            continue;
        }
        let start = if first {
            literal_start.unwrap_or(left)
        } else {
            left
        };
        first = false;
        result.push(InlineNode::RawText(Raw {
            content: &text[left..right],
            location: locate(text, fragments, start..right, state),
            subs: raw
                .subs
                .iter()
                .filter(|substitution| {
                    literal_start.is_none() || **substitution != Substitution::Replacements
                })
                .cloned()
                .collect(),
        }));
    }
}

fn locate(
    text: &str,
    fragments: &[Fragment<'_, '_>],
    range: Range<usize>,
    state: &ParserState<'_>,
) -> Location {
    let start = fragment_offset(range.start, fragments, false, state);
    let end = fragment_offset(
        step_char(text, range.end, RoundDirection::Backward),
        fragments,
        true,
        state,
    );
    state.create_location(start, end)
}
