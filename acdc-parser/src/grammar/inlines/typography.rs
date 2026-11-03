mod encoded_arrows;
mod neighbors;

use std::{mem::take, ops::Range};

use crate::{
    IndexTermKind, InlineMacro, InlineNode, Location, Plain, Raw, Substitution, Symbol, SymbolKind,
    grammar::{
        ParserState,
        state::InlineRules,
        utf8_utils::{RoundDirection, step_char},
    },
    model::substitution::SubstitutionPlan,
};

use super::recognition::structural_token_allowed;

pub(crate) fn resolve_plain<'a>(
    nodes: Vec<InlineNode<'a>>,
    state: &ParserState<'a>,
    paragraph: bool,
) -> Vec<InlineNode<'a>> {
    resolve_plain_with_context(nodes, state, paragraph, WordContext::default())
}

fn resolve_plain_with_context<'a>(
    nodes: Vec<InlineNode<'a>>,
    state: &ParserState<'a>,
    paragraph: bool,
    context: WordContext,
) -> Vec<InlineNode<'a>> {
    if !state
        .inline_ctx
        .substitutions
        .enabled(&Substitution::Replacements)
        || state.inline_ctx.rules.contains(InlineRules::INDEX_CATALOG)
        || !nodes.iter().any(neighbors::needs_resolution)
    {
        return nodes;
    }
    let words = neighbors::contexts(&nodes, state, context);
    let mut result = Vec::with_capacity(nodes.len());
    for (index, mut node) in nodes.into_iter().enumerate() {
        let words = words
            .as_ref()
            .and_then(|words| words.get(index))
            .copied()
            .unwrap_or_default();
        if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = &mut node
            && matches!(&term.kind, IndexTermKind::Flow(nodes) if nodes.iter().any(neighbors::needs_resolution))
        {
            let original = term.catalog.is_none().then(|| Box::new((**term).clone()));
            if let IndexTermKind::Flow(nodes) = &mut term.kind {
                *nodes = resolve_plain_with_context(take(nodes), state, false, words);
            }
            if let Some(original) = original
                && term.kind != original.kind
            {
                term.catalog = Some(original);
            }
            result.push(node);
            continue;
        }
        let InlineNode::PlainText(text) = node else {
            result.push(node);
            continue;
        };
        if text.escaped {
            result.push(InlineNode::PlainText(text));
            continue;
        }
        let start = source_start(text.content, &text.location, state);
        let edges = Edges {
            start: paragraph && start == Some(0) && !late_boundary(state, 0),
            end: paragraph
                && start.is_some_and(|start| start + text.content.len() == state.input.len())
                && !late_boundary(state, state.input.len()),
        };
        let edits = replacements(text.content, edges, words, |range, kind| {
            arrow_allowed(
                text.content,
                range.clone(),
                kind,
                state.inline_ctx.substitutions,
            ) && start.is_none_or(|start| {
                structural_token_allowed(
                    state,
                    &Substitution::Replacements,
                    start + range.start,
                    range.len(),
                )
            })
        });
        if edits.is_empty() {
            result.push(InlineNode::PlainText(text));
        } else {
            append_resolved(
                &mut result,
                text.content,
                &text.location,
                start,
                edits,
                None,
                state,
            );
        }
    }
    result
}

pub(crate) fn resolve_raw<'a>(
    raw: Raw<'a>,
    state: &ParserState<'a>,
    edges: Edges,
) -> Vec<InlineNode<'a>> {
    if !raw.subs.contains(&Substitution::Replacements) {
        return vec![InlineNode::RawText(raw)];
    }
    let plan = SubstitutionPlan::from_substitutions(&raw.subs);
    let edits = replacements(raw.content, edges, WordContext::default(), |range, kind| {
        arrow_allowed(raw.content, range, kind, plan)
    });
    if edits.is_empty() {
        return vec![InlineNode::RawText(raw)];
    }
    let mut result = Vec::new();
    let start = source_start(raw.content, &raw.location, state);
    append_resolved(
        &mut result,
        raw.content,
        &raw.location,
        start,
        edits,
        Some(&raw.subs),
        state,
    );
    result
}

fn has_candidate(text: &str) -> bool {
    text.contains("(C)")
        || text.contains("(R)")
        || text.contains("(TM)")
        || text.contains("...")
        || text.contains("--")
        || text.contains('\'')
        || ["->", "=>", "<-", "<="]
            .iter()
            .any(|token| text.contains(token))
        || text.contains('&')
            && [
                "-&gt;",
                "=&gt;",
                "&lt;-",
                "&lt;=",
                r"&lt;\-&gt;",
                r"&lt;\=&gt;",
            ]
            .iter()
            .any(|token| text.contains(token))
}

pub(crate) struct Replacement {
    pub(crate) range: Range<usize>,
    pub(crate) value: ReplacementValue,
}

pub(crate) enum ReplacementValue {
    Symbol(SymbolKind),
    Literal(Range<usize>),
    EncodedLiteral {
        range: Range<usize>,
        decoded: &'static str,
    },
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Edges {
    pub(crate) start: bool,
    pub(crate) end: bool,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct WordContext {
    before: Option<(char, bool)>,
    after: Option<(char, bool)>,
}

fn late_boundary(state: &ParserState<'_>, position: usize) -> bool {
    state
        .inline_ctx
        .substitutions
        .precedes(&Substitution::Replacements, &Substitution::Attributes)
        && state
            .late_attribute_sources
            .iter()
            .any(|(range, _)| range.start == position && range.end == position)
}

pub(crate) fn replacements(
    text: &str,
    edges: Edges,
    words: WordContext,
    allowed: impl Fn(Range<usize>, SymbolKind) -> bool,
) -> Vec<Replacement> {
    let mut edits = Vec::new();
    let mut cursor = 0;
    while cursor < text.len() {
        let escaped = text.as_bytes().get(cursor) == Some(&b'\\');
        let start = cursor + usize::from(escaped);
        let Some((mut range, kind)) = replacement_at(text, start, edges, words) else {
            cursor += text[cursor..].chars().next().map_or(1, char::len_utf8);
            continue;
        };
        if escaped
            && !allowed(
                cursor..if is_arrow(kind) { range.end } else { start + 1 },
                kind,
            )
        {
            cursor = start;
            continue;
        }
        let end = range.end;
        if edits
            .last()
            .is_some_and(|previous: &Replacement| previous.range.end > range.start)
        {
            cursor = start + 1;
            continue;
        }
        if escaped {
            range.start = cursor;
        }
        let mut context = range.clone();
        let mut word_context_allowed = true;
        if matches!(
            kind,
            SymbolKind::CurvedApostrophe | SymbolKind::WordBoundedEmDash
        ) {
            word_context_allowed = (range.start != 0
                || words.before.is_none_or(|(_, allowed)| allowed))
                && (range.end != text.len() || words.after.is_none_or(|(_, allowed)| allowed));
            context.start -= text[..range.start]
                .chars()
                .next_back()
                .map_or(0, char::len_utf8);
            context.end += text[range.end..].chars().next().map_or(0, char::len_utf8);
        }
        let value = if !word_context_allowed || !allowed(context, kind) {
            ReplacementValue::Literal(range.clone())
        } else if escaped {
            ReplacementValue::Literal(start..end)
        } else {
            ReplacementValue::Symbol(kind)
        };
        encoded_arrows::append_edits(text, range, value, &mut edits, &allowed);
        cursor = edits.last().map_or(end, |edit| edit.range.end);
    }
    edits
}

fn replacement_at(
    text: &str,
    start: usize,
    edges: Edges,
    words: WordContext,
) -> Option<(Range<usize>, SymbolKind)> {
    let suffix = &text[start..];
    if let Some(arrow) = encoded_arrows::at(suffix) {
        return Some((start..start + arrow.0, arrow.1));
    }
    for (token, kind) in [
        ("(C)", SymbolKind::Copyright),
        ("(R)", SymbolKind::RegisteredTrademark),
        ("(TM)", SymbolKind::Trademark),
        ("...", SymbolKind::Ellipsis),
    ] {
        if suffix.starts_with(token) {
            return Some((start..start + token.len(), kind));
        }
    }
    for (token, kind) in [
        ("=>", SymbolKind::RightDoubleArrow),
        ("->", SymbolKind::RightArrow),
        ("<=", SymbolKind::LeftDoubleArrow),
        ("<-", SymbolKind::LeftArrow),
    ] {
        if suffix.starts_with(token) && !(token.starts_with('<') && suffix[2..].starts_with('>')) {
            return Some((start..start + token.len(), kind));
        }
    }
    let previous = text[..start]
        .strip_suffix('\\')
        .unwrap_or(&text[..start])
        .chars()
        .next_back();
    if suffix.starts_with('\'')
        && previous
            .or(words.before.map(|(character, _)| character))
            .is_some_and(char::is_alphanumeric)
        && suffix[1..]
            .chars()
            .next()
            .or(words.after.map(|(character, _)| character))
            .is_some_and(char::is_alphabetic)
    {
        return Some((start..start + 1, SymbolKind::CurvedApostrophe));
    }
    if suffix.starts_with("--") && !suffix.starts_with("---") && previous != Some('-') {
        let next = suffix[2..].chars().next();
        if text[..start].ends_with('\\')
            || previous
                .or(words.before.map(|(character, _)| character))
                .is_some_and(is_word)
                && next
                    .or(words.after.map(|(character, _)| character))
                    .is_some_and(is_word)
        {
            return Some((start..start + 2, SymbolKind::WordBoundedEmDash));
        }
        if (previous.is_some_and(|character| matches!(character, ' ' | '\n'))
            || start == 0 && edges.start)
            && (next.is_some_and(|character| matches!(character, ' ' | '\n'))
                || start + 2 == text.len() && edges.end)
        {
            return Some((
                start.saturating_sub(1)..(start + 3).min(text.len()),
                SymbolKind::SpacedEmDash,
            ));
        }
    }
    None
}

pub(crate) fn arrow_allowed(
    text: &str,
    range: Range<usize>,
    kind: SymbolKind,
    plan: SubstitutionPlan,
) -> bool {
    !is_arrow(kind)
        || text[range].contains('&')
            != plan.precedes(&Substitution::SpecialChars, &Substitution::Replacements)
}

pub(crate) fn is_arrow(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::RightArrow
            | SymbolKind::LeftArrow
            | SymbolKind::RightDoubleArrow
            | SymbolKind::LeftDoubleArrow
    )
}

fn is_word(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn source_start(text: &str, location: &Location, state: &ParserState<'_>) -> Option<usize> {
    let end = step_char(state.input, location.absolute_end, RoundDirection::Forward);
    let original = state.input.get(location.absolute_start..end)?;
    let prefix = original.strip_suffix(text)?;
    prefix
        .bytes()
        .all(|byte| byte == b'\\')
        .then_some(location.absolute_start + prefix.len())
}

fn append_resolved<'a>(
    result: &mut Vec<InlineNode<'a>>,
    text: &'a str,
    location: &Location,
    source_start: Option<usize>,
    edits: Vec<Replacement>,
    raw_subs: Option<&[Substitution]>,
    state: &ParserState<'a>,
) {
    let locate = |range: Range<usize>| {
        source_start.map_or_else(
            || location.clone(),
            |start| {
                let first = if range.start == 0 {
                    location.absolute_start
                } else {
                    start + range.start
                };
                state.create_block_location(first, start + range.end, 0)
            },
        )
    };
    let plain = |range: Range<usize>| {
        if let Some(subs) = raw_subs {
            InlineNode::RawText(Raw {
                content: &text[range.clone()],
                location: locate(range),
                subs: subs.to_vec(),
            })
        } else {
            InlineNode::PlainText(Plain {
                content: &text[range.clone()],
                location: locate(range),
                escaped: false,
            })
        }
    };
    let mut cursor = 0;
    for edit in edits {
        if cursor < edit.range.start {
            result.push(plain(cursor..edit.range.start));
        }
        let node_location = locate(edit.range.clone());
        result.push(match edit.value {
            ReplacementValue::Symbol(kind) => InlineNode::Symbol(Symbol {
                kind,
                location: node_location,
            }),
            ReplacementValue::EncodedLiteral { range, decoded } => {
                let subs = raw_subs.unwrap_or(&[]);
                if subs.contains(&Substitution::SpecialChars)
                    || raw_subs.is_none()
                        && state
                            .inline_ctx
                            .substitutions
                            .enabled(&Substitution::SpecialChars)
                {
                    InlineNode::RawText(Raw {
                        content: &text[range],
                        location: node_location,
                        subs: vec![Substitution::SpecialChars],
                    })
                } else {
                    InlineNode::RawText(Raw {
                        content: decoded,
                        location: node_location,
                        subs: vec![Substitution::SpecialChars],
                    })
                }
            }
            ReplacementValue::Literal(range) => {
                protected_literal(text, range, node_location, raw_subs, state)
            }
        });
        cursor = edit.range.end;
    }
    if cursor < text.len() {
        result.push(plain(cursor..text.len()));
    }
}

fn protected_literal<'a>(
    text: &'a str,
    range: Range<usize>,
    location: Location,
    raw_subs: Option<&[Substitution]>,
    state: &ParserState<'a>,
) -> InlineNode<'a> {
    let literal = &text[range];
    let content = if raw_subs.is_none()
        && state
            .inline_ctx
            .substitutions
            .precedes(&Substitution::SpecialChars, &Substitution::Replacements)
        && literal.contains('&')
    {
        state.intern_str(&literal.replace("&lt;", "<").replace("&gt;", ">"))
    } else {
        literal
    };
    InlineNode::RawText(Raw {
        content,
        location,
        subs: raw_subs.map_or_else(
            || {
                state
                    .inline_ctx
                    .substitutions
                    .enabled(&Substitution::SpecialChars)
                    .then_some(Substitution::SpecialChars)
                    .into_iter()
                    .collect()
            },
            |subs| {
                subs.iter()
                    .filter(|substitution| **substitution != Substitution::Replacements)
                    .cloned()
                    .collect()
            },
        ),
    })
}
