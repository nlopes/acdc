//! Candidate offsets keep registration lookup out of the character scanner.
//! Only the inline grammar can commit a candidate at an eligible node boundary.

use std::{cell::Cell, sync::Arc};

use crate::{
    InlineNode, Plain, Substitution,
    extensions::{InlineMatcher, RetainedInline, late_inline::Arguments},
    grammar::{ParserState, brackets::escaped_bracket_end},
};

#[derive(Debug)]
struct Candidate {
    start: usize,
    prefix: usize,
    registration: usize,
}

#[derive(Debug)]
pub(in crate::grammar) struct Candidates<'a> {
    offsets: Vec<Candidate>,
    plain_end: Cell<usize>,
    attributes: Arc<crate::DocumentAttributes<'a>>,
}

pub(in crate::grammar) fn candidates<'a>(state: &ParserState<'a>) -> Option<Box<Candidates<'a>>> {
    let mut found = Vec::new();
    if !state
        .inline_ctx
        .substitutions
        .enabled(&Substitution::Macros)
        || state.quotes_only
    {
        return None;
    }
    let Some(registry) = &state.options.extensions else {
        return None;
    };
    let input = state.input;
    let mut record = |prefix: usize, registration| {
        let start = prefix - usize::from(input[..prefix].ends_with('\\'));
        found.push(Candidate {
            start,
            prefix,
            registration,
        });
    };
    for (colon, _) in input.match_indices(':') {
        let name_start = input[..colon]
            .trim_end_matches(|c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
            .len();
        if let Some(registration) = registry.named_inline(&input[name_start..colon]) {
            record(name_start, registration);
        }
    }
    for (registration, entry) in registry.custom_inlines() {
        for (prefix, _) in input.match_indices(&entry.prefix) {
            record(prefix, registration);
        }
    }
    found.sort_by_key(|c| (c.start, c.registration));
    (!found.is_empty()).then(|| {
        Box::new(Candidates {
            offsets: found,
            plain_end: Cell::new(0),
            attributes: Arc::new((*state.document_attributes).clone()),
        })
    })
}

/// Split one complete invocation at an eligible grammar position.
pub(in crate::grammar) fn arguments<'a>(
    entry: &crate::extensions::late_inline::Registration,
    source: &'a str,
) -> Option<(&'a str, &'a str, usize)> {
    let tail = source.strip_prefix(&entry.prefix)?;
    let (target, body, end) = match &entry.matcher {
        InlineMatcher::Delimited { close, .. } => {
            let end = tail.find(close)?;
            ("", &tail[..end], end + close.len())
        }
        InlineMatcher::Full | InlineMatcher::Short => {
            if tail.starts_with(':') {
                return None;
            }
            let open = tail.find(|c: char| c == '[' || c.is_whitespace())?;
            if tail.as_bytes().get(open) != Some(&b'[')
                || (open == 0) != (entry.matcher == InlineMatcher::Short)
            {
                return None;
            }
            let end = escaped_bracket_end(tail, open + 1)?;
            {
                let body = &tail[open + 1..end];
                (if open == 0 { body } else { &tail[..open] }, body, end + 1)
            }
        }
    };
    Some((target, body, end + entry.prefix.len()))
}

pub(super) fn recognize<'a>(
    state: &ParserState<'a>,
    start: usize,
) -> peg::RuleResult<InlineNode<'a>> {
    let Some(registry) = &state.options.extensions else {
        return peg::RuleResult::Failed;
    };
    let Some(candidates) = &state.inline_candidates else {
        return peg::RuleResult::Failed;
    };
    let index = candidates.offsets.partition_point(|c| c.start < start);
    for candidate in candidates
        .offsets
        .iter()
        .skip(index)
        .take_while(|c| c.start == start)
    {
        let Some(entry) = registry.inline_macros().get(candidate.registration) else {
            continue;
        };
        if !super::recognition::macro_token_allowed(state, candidate.prefix, entry.prefix.len()) {
            continue;
        }
        let Some((target, body, len)) = arguments(entry, &state.input[candidate.prefix..]) else {
            continue;
        };
        let end = candidate.prefix + len;
        let literal = Plain::new(
            &state.input[candidate.prefix..end],
            state.create_block_location(start, end, state.inline_ctx.offset),
        )
        .with_escaped(true);
        let node = if candidate.prefix == start {
            if let Some(retained) = &state.retained_inline {
                retained.set(true);
            }
            InlineNode::Extension(Box::new(RetainedInline {
                literal,
                args: Arc::new(Arguments {
                    file: state.current_file.as_deref().cloned(),
                    name: entry.name().to_owned(),
                    target,
                    body,
                    substitute_attributes: state
                        .inline_ctx
                        .substitutions
                        .enabled(&Substitution::Attributes),
                    attributes: Arc::clone(&candidates.attributes),
                }),
            }))
        } else {
            InlineNode::Plain(literal)
        };
        return peg::RuleResult::Matched(end, node);
    }
    // Reuse the remainder of an already scanned plain run between calls.
    let limit = candidates.plain_end.get();
    if start < limit {
        let next = candidates.offsets.partition_point(|c| c.start <= start);
        let end = candidates
            .offsets
            .get(next)
            .map_or(limit, |c| limit.min(c.start));
        return peg::RuleResult::Matched(
            end,
            InlineNode::Plain(Plain::new(
                &state.input[start..end],
                state.create_block_location(start, end, state.inline_ctx.offset),
            )),
        );
    }
    peg::RuleResult::Failed
}

/// An ordinary text run can cross a registered prefix. Leave the suffix to PEG;
/// formatted spans and built-in macros keep ownership of their entire match.
pub(super) fn clip<'a>(
    state: &ParserState<'a>,
    start: usize,
    end: usize,
    node: InlineNode<'a>,
) -> peg::RuleResult<InlineNode<'a>> {
    if let Some(candidates) = &state.inline_candidates
        && matches!(&node, InlineNode::Plain(plain) if !plain.escaped && state.input.get(start..end).is_some_and(|source| std::ptr::eq(plain.content,source)))
    {
        let index = candidates.offsets.partition_point(|c| c.start <= start);
        if let Some(call) = candidates.offsets.get(index).filter(|c| c.start < end) {
            candidates.plain_end.set(end);
            let location = state.create_block_location(start, call.start, state.inline_ctx.offset);
            return peg::RuleResult::Matched(
                call.start,
                InlineNode::Plain(Plain::new(&state.input[start..call.start], location)),
            );
        }
    }
    peg::RuleResult::Matched(end, node)
}
