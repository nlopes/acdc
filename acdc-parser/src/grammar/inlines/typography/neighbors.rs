use crate::{IndexTermKind, InlineMacro, InlineNode, Location, Substitution, grammar::ParserState};

use super::{WordContext, has_candidate, source_start, structural_token_allowed};

pub(super) fn needs_resolution(node: &InlineNode<'_>, ordinary: bool) -> bool {
    if let InlineNode::PlainText(text) = node {
        return ordinary && !text.escaped && has_candidate(text.content);
    }
    if let InlineNode::RawText(raw) = node {
        return raw.subs.contains(&Substitution::Replacements) && needs_context(raw.content);
    }
    if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node {
        return matches!(&term.kind, IndexTermKind::Flow(nodes) if nodes.iter().any(|node| needs_resolution(node, ordinary)));
    }
    false
}

pub(super) fn needs_word_context(node: &InlineNode<'_>) -> bool {
    if let InlineNode::PlainText(text) = node {
        return !text.escaped && needs_context(text.content);
    }
    if let InlineNode::RawText(raw) = node {
        return raw.subs.contains(&Substitution::Replacements) && needs_context(raw.content);
    }
    if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node {
        return matches!(&term.kind, IndexTermKind::Flow(nodes) if nodes.iter().any(needs_word_context));
    }
    false
}

fn needs_context(text: &str) -> bool {
    ["'", "-"]
        .iter()
        .any(|token| text.starts_with(token) || text.ends_with(token))
}

pub(super) fn contexts(
    nodes: &[InlineNode<'_>],
    state: &ParserState<'_>,
    inherited: WordContext,
) -> Option<Vec<WordContext>> {
    if !nodes.iter().any(needs_word_context) {
        return None;
    }
    let mut contexts = vec![WordContext::default(); nodes.len()];
    let mut before = inherited.before;
    for (node, context) in nodes.iter().zip(&mut contexts) {
        context.before = before;
        before = edge(node, false, state).update(before);
    }
    let mut after = inherited.after;
    for (node, context) in nodes.iter().zip(&mut contexts).rev() {
        context.after = after;
        after = edge(node, true, state).update(after);
    }
    Some(contexts)
}

enum Edge {
    Text(char, bool),
    Transparent,
    Boundary,
}

impl Edge {
    fn update(self, inherited: Option<(char, bool)>) -> Option<(char, bool)> {
        match self {
            Self::Text(character, allowed) => Some((character, allowed)),
            Self::Transparent => inherited,
            Self::Boundary => None,
        }
    }
}

fn edge(node: &InlineNode<'_>, first: bool, state: &ParserState<'_>) -> Edge {
    match node {
        InlineNode::PlainText(text) => text_edge(text.content, &text.location, first, state),
        InlineNode::RawText(raw) => text_edge(raw.content, &raw.location, first, state),
        InlineNode::Symbol(symbol) => {
            let text = symbol.kind.text();
            let character = if first {
                text.chars().next()
            } else {
                text.chars().next_back()
            };
            character.map_or(Edge::Transparent, |character| Edge::Text(character, true))
        }
        InlineNode::InlineAnchor(anchor) if !anchor.is_bibliography() => Edge::Transparent,
        InlineNode::Macro(InlineMacro::IndexTerm(term)) => {
            let IndexTermKind::Flow(nodes) = &term.kind else {
                return Edge::Transparent;
            };
            let mut nodes = nodes.iter();
            while let Some(node) = if first {
                nodes.next()
            } else {
                nodes.next_back()
            } {
                let edge = edge(node, first, state);
                if !matches!(edge, Edge::Transparent) {
                    return edge;
                }
            }
            Edge::Transparent
        }
        InlineNode::VerbatimText(_)
        | InlineNode::BoldText(_)
        | InlineNode::ItalicText(_)
        | InlineNode::MonospaceText(_)
        | InlineNode::HighlightText(_)
        | InlineNode::SubscriptText(_)
        | InlineNode::SuperscriptText(_)
        | InlineNode::DoubleQuotedSpan(_)
        | InlineNode::SingleQuotedSpan(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::Macro(_)
        | InlineNode::CalloutMarker(_)
        | InlineNode::CharacterReference(_) => Edge::Boundary,
    }
}

fn text_edge(text: &str, location: &Location, first: bool, state: &ParserState<'_>) -> Edge {
    let character = if first {
        text.char_indices().next()
    } else {
        text.char_indices().next_back()
    };
    let Some((offset, character)) = character else {
        return Edge::Transparent;
    };
    let allowed = !state
        .inline_ctx
        .substitutions
        .precedes(&Substitution::Replacements, &Substitution::Attributes)
        || source_start(text, location, state).is_some_and(|start| {
            structural_token_allowed(
                state,
                &Substitution::Replacements,
                start + offset,
                character.len_utf8(),
            )
        });
    Edge::Text(character, allowed)
}

pub(super) fn protected_edge(
    node: &InlineNode<'_>,
    first: bool,
    state: &ParserState<'_>,
) -> Option<(char, bool)> {
    if !matches!(node, InlineNode::RawText(_) | InlineNode::PlainText(_)) {
        return None;
    }
    if let Edge::Text(character, allowed) = edge(node, first, state) {
        Some((character, allowed))
    } else {
        None
    }
}
