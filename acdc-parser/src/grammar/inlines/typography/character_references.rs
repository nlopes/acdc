use std::ops::Range;

use crate::{
    CharacterReference, CharacterReferenceKind, IndexTermKind, InlineMacro, InlineNode, Location,
    Plain, Raw, Substitution,
    grammar::{
        ParserState, character_references::body, inlines::recognition::byte_came_from_attribute,
        state::InlineRules,
    },
    model::substitution::SubstitutionPlan,
};

use super::{source_start, structural_token_allowed};

pub(super) fn resolve<'a>(
    nodes: Vec<InlineNode<'a>>,
    state: &ParserState<'a>,
) -> Vec<InlineNode<'a>> {
    let ordinary = state
        .inline_ctx
        .substitutions
        .enabled(&Substitution::Replacements);
    if state.inline_ctx.rules.contains(InlineRules::INDEX_CATALOG)
        || !nodes.iter().any(|node| needed(node, ordinary))
    {
        return nodes;
    }
    let mut result = Vec::with_capacity(nodes.len());
    for mut node in nodes {
        if let InlineNode::PlainText(text) = node {
            if ordinary && text.content.contains('&') {
                resolve_text(
                    &mut result,
                    text.content,
                    &text.location,
                    None,
                    text.escaped,
                    state,
                );
            } else {
                result.push(InlineNode::PlainText(text));
            }
        } else if let InlineNode::RawText(raw) = node {
            if raw.content.contains('&') && raw.subs.contains(&Substitution::Replacements) {
                resolve_text(
                    &mut result,
                    raw.content,
                    &raw.location,
                    Some(&raw.subs),
                    false,
                    state,
                );
            } else {
                result.push(InlineNode::RawText(raw));
            }
        } else {
            if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = &mut node
                && matches!(&term.kind, IndexTermKind::Flow(content) if content.iter().any(|node| needed(node, ordinary)))
            {
                let original = term.catalog.is_none().then(|| Box::new((**term).clone()));
                if let IndexTermKind::Flow(content) = &mut term.kind {
                    *content = resolve(std::mem::take(content), state);
                }
                if let Some(original) = original
                    && term.kind != original.kind
                {
                    term.catalog = Some(original);
                }
            }
            result.push(node);
        }
    }
    result
}

fn needed(node: &InlineNode<'_>, ordinary: bool) -> bool {
    if let InlineNode::PlainText(text) = node {
        return ordinary && text.content.contains('&');
    }
    if let InlineNode::RawText(raw) = node {
        return raw.subs.contains(&Substitution::Replacements) && raw.content.contains('&');
    }
    if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node {
        return matches!(&term.kind, IndexTermKind::Flow(content) if content.iter().any(|node| needed(node, ordinary)));
    }
    false
}

struct Reference<'a> {
    range: Range<usize>,
    value: Value<'a>,
}

enum Value<'a> {
    Reference(CharacterReferenceKind<'a>),
    Literal(&'a str, bool),
}

fn resolve_text<'a>(
    result: &mut Vec<InlineNode<'a>>,
    text: &'a str,
    location: &Location,
    raw_subs: Option<&[Substitution]>,
    escaped: bool,
    state: &ParserState<'a>,
) {
    let plan = raw_subs.map_or(
        state.inline_ctx.substitutions,
        SubstitutionPlan::from_substitutions,
    );
    let start = source_start(text, location, state);
    let locate = |range: Range<usize>| {
        start.map_or_else(
            || location.clone(),
            |start| {
                state.create_block_location(
                    if range.start == 0 {
                        location.absolute_start
                    } else {
                        start + range.start
                    },
                    start + range.end,
                    0,
                )
            },
        )
    };
    let fragment = |range: Range<usize>| {
        if let Some(subs) = raw_subs {
            InlineNode::RawText(Raw {
                metadata: None,
                content: &text[range.clone()],
                location: locate(range),
                subs: subs.to_vec(),
            })
        } else {
            InlineNode::PlainText(Plain {
                content: &text[range.clone()],
                location: locate(range),
                escaped,
            })
        }
    };
    let mut cursor = 0;
    let mut search = 0;
    while let Some(relative) = text[search..].find('&') {
        let amp = search + relative;
        let Some(reference) = reference_at(text, amp, start, plan, raw_subs.is_none(), state)
        else {
            search = amp + 1;
            continue;
        };
        if cursor < reference.range.start {
            result.push(fragment(cursor..reference.range.start));
        }
        let location = locate(reference.range.clone());
        match reference.value {
            Value::Reference(kind) => {
                if let Some(value) = CharacterReference::new(kind, location) {
                    result.push(InlineNode::CharacterReference(value));
                } else {
                    result.push(fragment(reference.range.clone()));
                }
            }
            Value::Literal(content, escaping) => result.push(InlineNode::RawText(Raw {
                metadata: None,
                content,
                location,
                subs: if escaping {
                    vec![Substitution::SpecialChars]
                } else {
                    Vec::new()
                },
            })),
        }
        cursor = reference.range.end;
        search = cursor;
    }
    if cursor < text.len() {
        result.push(fragment(cursor..text.len()));
    }
}

fn reference_at<'a>(
    text: &'a str,
    amp: usize,
    source_start: Option<usize>,
    plan: SubstitutionPlan,
    ordinary: bool,
    state: &ParserState<'a>,
) -> Option<Reference<'a>> {
    let from_attribute =
        source_start.is_some_and(|start| byte_came_from_attribute(state, start + amp));
    if ordinary && from_attribute {
        return None;
    }
    let special = plan.enabled(&Substitution::SpecialChars);
    let before = special && plan.precedes(&Substitution::SpecialChars, &Substitution::Replacements);
    let after = special && plan.precedes(&Substitution::Replacements, &Substitution::SpecialChars);
    let encoded = !before && text[amp..].starts_with("&amp;");
    let body_start = amp + if encoded { 5 } else { 1 };
    let (kind, length) = body(&text[body_start..], state.options.compatibility_mode)?;
    let end = body_start + length;
    let escaped = (before || encoded) && amp > 0 && text.as_bytes().get(amp - 1) == Some(&b'\\');
    let start = amp - usize::from(escaped);
    if ordinary
        && source_start.is_some_and(|base| {
            !structural_token_allowed(
                state,
                &Substitution::Replacements,
                base + start,
                end - start,
            )
        })
    {
        return None;
    }
    let value = if escaped {
        Value::Literal(&text[amp..end], special)
    } else if after {
        if !encoded {
            return None;
        }
        Value::Literal(
            state.intern_fmt(format_args!("&{}", &text[body_start..end])),
            true,
        )
    } else {
        Value::Reference(kind)
    };
    Some(Reference {
        range: start..end,
        value,
    })
}
