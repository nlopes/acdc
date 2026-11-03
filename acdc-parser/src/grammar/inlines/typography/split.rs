use std::{collections::BTreeMap, mem::take, ops::Range};

use crate::{
    IndexTermKind, InlineMacro, InlineNode, Plain, Substitution,
    grammar::{ParserState, state::InlineRules},
};

use super::{
    Edges, Replacement, ReplacementValue, WordContext, append_resolved, late_boundary, neighbors,
    replacements, source_start, structural_token_allowed,
};

struct Fragment<'tree, 'source> {
    node: usize,
    range: Range<usize>,
    plain: &'tree Plain<'source>,
}

struct Planner<'tree, 'source, 'state> {
    state: &'state ParserState<'source>,
    text: String,
    fragments: Vec<Fragment<'tree, 'source>>,
    parents: Vec<Option<usize>>,
    changes: BTreeMap<usize, Vec<Replacement>>,
    at_start: bool,
    before: Option<(char, bool)>,
}

pub(super) fn resolve<'a>(
    nodes: Vec<InlineNode<'a>>,
    state: &ParserState<'a>,
    paragraph: bool,
) -> Vec<InlineNode<'a>> {
    if state.inline_ctx.rules.contains(InlineRules::INDEX_CATALOG)
        || !has_dash(&nodes)
        || !state
            .inline_ctx
            .substitutions
            .enabled(&Substitution::Replacements)
        || !nodes.iter().any(|node| {
            matches!(node, InlineNode::InlineAnchor(anchor) if !anchor.is_bibliography())
                || matches!(node, InlineNode::Macro(InlineMacro::IndexTerm(_)))
        })
    {
        return nodes;
    }
    let (mut changes, parents) = {
        let mut planner = Planner {
            state,
            text: String::new(),
            fragments: Vec::new(),
            parents: Vec::new(),
            changes: BTreeMap::new(),
            at_start: paragraph,
            before: None,
        };
        planner.collect(&nodes, None);
        planner.flush(paragraph, None);
        (planner.changes, planner.parents)
    };
    if changes.is_empty() {
        return nodes;
    }
    let mut snapshots = vec![false; parents.len()];
    for &node in changes.keys() {
        let mut parent = parents.get(node).copied().flatten();
        while let Some(index) = parent {
            if let Some(snapshot) = snapshots.get_mut(index) {
                *snapshot = true;
            }
            parent = parents.get(index).copied().flatten();
        }
    }
    let mut next = 0;
    let result = apply(nodes, &mut changes, &snapshots, &mut next, state);
    debug_assert!(changes.is_empty());
    result
}

fn has_dash(nodes: &[InlineNode<'_>]) -> bool {
    nodes.iter().any(|node| {
        if let InlineNode::PlainText(plain) = node {
            return !plain.escaped && plain.content.contains('-');
        }
        if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node {
            return matches!(&term.kind, IndexTermKind::Flow(nodes) if has_dash(nodes));
        }
        false
    })
}

impl<'tree, 'source> Planner<'tree, 'source, '_> {
    fn collect(&mut self, nodes: &'tree [InlineNode<'source>], parent: Option<usize>) {
        for node in nodes {
            let index = self.parents.len();
            self.parents.push(parent);
            if let InlineNode::PlainText(plain) = node
                && !plain.escaped
            {
                let start = self.text.len();
                self.text.push_str(plain.content);
                self.fragments.push(Fragment {
                    node: index,
                    range: start..self.text.len(),
                    plain,
                });
                continue;
            }
            if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node {
                if let IndexTermKind::Flow(nodes) = &term.kind {
                    self.collect(nodes, Some(index));
                }
                continue;
            }
            if matches!(node, InlineNode::InlineAnchor(anchor) if !anchor.is_bibliography()) {
                continue;
            }
            let line_break = matches!(node, InlineNode::LineBreak(_));
            self.flush(
                line_break,
                neighbors::protected_edge(node, true, self.state),
            );
            self.at_start = line_break;
            self.before = neighbors::protected_edge(node, false, self.state);
        }
    }

    fn flush(&mut self, at_end: bool, after: Option<(char, bool)>) {
        if self.text.contains("--") {
            let edges = Edges {
                start: self.at_start
                    && self.fragments.first().is_none_or(|fragment| {
                        source_start(fragment.plain.content, &fragment.plain.location, self.state)
                            .is_none_or(|start| !late_boundary(self.state, start))
                    }),
                end: at_end
                    && self.fragments.last().is_none_or(|fragment| {
                        source_start(fragment.plain.content, &fragment.plain.location, self.state)
                            .is_none_or(|start| {
                                !late_boundary(self.state, start + fragment.plain.content.len())
                            })
                    }),
            };
            let edits = replacements(
                &self.text,
                edges,
                WordContext {
                    before: self.before,
                    after,
                },
                |range, _| self.allowed(range),
            );
            for edit in edits {
                if self.text[edit.range.clone()].contains("--") {
                    self.record(&edit);
                }
            }
        }
        self.text.clear();
        self.fragments.clear();
        self.at_start = false;
        self.before = None;
    }

    fn allowed(&self, range: Range<usize>) -> bool {
        if !self
            .state
            .inline_ctx
            .substitutions
            .precedes(&Substitution::Replacements, &Substitution::Attributes)
        {
            return true;
        }
        let first = self
            .fragments
            .partition_point(|fragment| fragment.range.end <= range.start);
        let end = self
            .fragments
            .partition_point(|fragment| fragment.range.start < range.end);
        let Some(covered) = self.fragments.get(first..end) else {
            return false;
        };
        covered.iter().all(|fragment| {
            source_start(fragment.plain.content, &fragment.plain.location, self.state).is_none_or(
                |start| {
                    let local = intersection(&range, &fragment.range);
                    structural_token_allowed(
                        self.state,
                        &Substitution::Replacements,
                        start + local.start - fragment.range.start,
                        local.len(),
                    )
                },
            )
        }) && !covered.windows(2).any(|pair| {
            let [left, right] = pair else {
                return false;
            };
            let left_end = source_start(left.plain.content, &left.plain.location, self.state)
                .map(|start| start + left.plain.content.len());
            let right_start = source_start(right.plain.content, &right.plain.location, self.state);
            self.state
                .empty_attribute_offsets
                .iter()
                .any(|offset| Some(*offset) == left_end || Some(*offset) == right_start)
        })
    }

    fn record(&mut self, edit: &Replacement) {
        let dash = self.text[edit.range.clone()]
            .find("--")
            .map(|offset| edit.range.start + offset);
        let first = self
            .fragments
            .partition_point(|fragment| fragment.range.end <= edit.range.start);
        for fragment in self
            .fragments
            .iter()
            .skip(first)
            .take_while(|fragment| fragment.range.start < edit.range.end)
        {
            let range = intersection(&edit.range, &fragment.range);
            let local = range.start - fragment.range.start..range.end - fragment.range.start;
            let value = match &edit.value {
                ReplacementValue::Symbol(kind)
                    if dash.is_some_and(|dash| fragment.range.contains(&dash)) =>
                {
                    ReplacementValue::Symbol(*kind)
                }
                ReplacementValue::Symbol(_) => ReplacementValue::Remove,
                ReplacementValue::Literal(literal) => {
                    let literal = intersection(literal, &fragment.range);
                    let start = literal
                        .start
                        .saturating_sub(fragment.range.start)
                        .min(fragment.plain.content.len());
                    let end = literal
                        .end
                        .saturating_sub(fragment.range.start)
                        .min(fragment.plain.content.len());
                    if start < end {
                        ReplacementValue::Literal(start..end)
                    } else {
                        ReplacementValue::Remove
                    }
                }
                ReplacementValue::EncodedLiteral { .. } | ReplacementValue::Remove => return,
            };
            self.changes
                .entry(fragment.node)
                .or_default()
                .push(Replacement {
                    range: local,
                    value,
                });
        }
    }
}

fn intersection(left: &Range<usize>, right: &Range<usize>) -> Range<usize> {
    left.start.max(right.start)..left.end.min(right.end)
}

fn apply<'a>(
    nodes: Vec<InlineNode<'a>>,
    changes: &mut BTreeMap<usize, Vec<Replacement>>,
    snapshots: &[bool],
    next: &mut usize,
    state: &ParserState<'a>,
) -> Vec<InlineNode<'a>> {
    let mut result = Vec::with_capacity(nodes.len());
    for mut node in nodes {
        let index = *next;
        *next += 1;
        if let InlineNode::PlainText(plain) = node {
            if let Some(edits) = changes.remove(&index) {
                let mut resolved = Vec::new();
                append_resolved(
                    &mut resolved,
                    plain.content,
                    &plain.location,
                    source_start(plain.content, &plain.location, state),
                    edits,
                    None,
                    state,
                );
                result.extend(resolved);
            } else {
                result.push(InlineNode::PlainText(plain));
            }
            continue;
        }
        if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = &mut node {
            let original = (term.catalog.is_none() && snapshots.get(index) == Some(&true))
                .then(|| Box::new((**term).clone()));
            if let IndexTermKind::Flow(nodes) = &mut term.kind {
                *nodes = apply(take(nodes), changes, snapshots, next, state);
            }
            if let Some(original) = original {
                term.catalog = Some(original);
            }
        }
        result.push(node);
    }
    result
}
