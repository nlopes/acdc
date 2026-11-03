use crate::{IndexTermKind, InlineMacro, InlineNode, Substitution, grammar::ParserState};

use super::{WordContext, has_candidate, source_start, structural_token_allowed};

pub(super) fn needs_resolution(node: &InlineNode<'_>) -> bool {
    if let InlineNode::PlainText(text) = node {
        return !text.escaped && has_candidate(text.content);
    }
    if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node {
        return matches!(&term.kind, IndexTermKind::Flow(nodes) if nodes.iter().any(needs_resolution));
    }
    false
}

fn needs_word_context(node: &InlineNode<'_>) -> bool {
    if let InlineNode::PlainText(text) = node {
        return !text.escaped
            && ["'", "--"]
                .iter()
                .any(|token| text.content.starts_with(token) || text.content.ends_with(token));
    }
    if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node {
        return matches!(&term.kind, IndexTermKind::Flow(nodes) if nodes.iter().any(needs_word_context));
    }
    false
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
        InlineNode::PlainText(text) => {
            let character = if first {
                text.content.char_indices().next()
            } else {
                text.content.char_indices().next_back()
            };
            let Some((offset, character)) = character else {
                return Edge::Transparent;
            };
            let allowed = source_start(text.content, &text.location, state).is_some_and(|start| {
                structural_token_allowed(
                    state,
                    &Substitution::Replacements,
                    start + offset,
                    character.len_utf8(),
                )
            });
            Edge::Text(character, allowed)
        }
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
        InlineNode::RawText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::BoldText(_)
        | InlineNode::ItalicText(_)
        | InlineNode::MonospaceText(_)
        | InlineNode::HighlightText(_)
        | InlineNode::SubscriptText(_)
        | InlineNode::SuperscriptText(_)
        | InlineNode::CurvedQuotationText(_)
        | InlineNode::CurvedApostropheText(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::Macro(_)
        | InlineNode::CalloutRef(_) => Edge::Boundary,
    }
}
