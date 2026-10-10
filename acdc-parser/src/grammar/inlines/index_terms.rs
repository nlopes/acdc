//! Index terms and their reference labels.

use crate::{
    IndexTerm, IndexTermKind, IndexTermRelationship, InlineMacro, InlineNode, Substitution,
    grammar::{
        ParserState, inline_parser,
        inlines::text::{map_registered_inline, registration_source, restored_label_offset},
        state::{InlineContext, InlineRules},
    },
    model::substitution::SubstitutionPlan,
};

#[derive(Clone, Copy)]
pub(super) struct IndexTermSegment<'a> {
    pub(super) text: &'a str,
    pub(super) start: usize,
}

#[derive(Clone)]
pub(super) enum IndexTermRelationshipSegments<'a> {
    See(IndexTermSegment<'a>),
    SeeAlso(Vec<IndexTermSegment<'a>>),
}

pub(super) enum IndexTermMacroItem<'a> {
    Term(IndexTermSegment<'a>),
    Relationship(IndexTermRelationshipSegments<'a>),
}

#[derive(Clone, Copy)]
pub(super) enum IndexTermForm {
    Flow,
    Concealed,
    NamedFlow,
    NamedConcealed,
}

impl IndexTermForm {
    fn visible(self) -> bool {
        matches!(self, Self::Flow | Self::NamedFlow)
    }
}

#[derive(Clone)]
struct IndexTermParts<'a> {
    terms: Vec<IndexTermSegment<'a>>,
    relationship: Option<IndexTermRelationshipSegments<'a>>,
}

pub(super) fn trimmed_index_term_segment(text: &str, start: usize) -> IndexTermSegment<'_> {
    let trimmed_start = text.trim_start();
    let leading = text.len() - trimmed_start.len();
    IndexTermSegment {
        text: trimmed_start.trim_end(),
        start: start + leading,
    }
}

fn split_index_term_relationship(
    segment: IndexTermSegment<'_>,
) -> (
    IndexTermSegment<'_>,
    Option<IndexTermRelationshipSegments<'_>>,
) {
    let segment = trimmed_index_term_segment(segment.text, segment.start);
    if let Some((term, target)) = segment.text.split_once(" >> ") {
        return (
            trimmed_index_term_segment(term, segment.start),
            Some(IndexTermRelationshipSegments::See(
                trimmed_index_term_segment(target, segment.start + term.len() + 4),
            )),
        );
    }
    if let Some((term, targets)) = segment.text.split_once(" &> ") {
        let mut start = segment.start + term.len() + 4;
        let targets = targets
            .split(" &> ")
            .map(|target| {
                let segment = trimmed_index_term_segment(target, start);
                start += target.len() + 4;
                segment
            })
            .collect();
        return (
            trimmed_index_term_segment(term, segment.start),
            Some(IndexTermRelationshipSegments::SeeAlso(targets)),
        );
    }
    (segment, None)
}

fn parse_index_term_inlines<'a>(
    state: &mut ParserState<'a>,
    segment: IndexTermSegment<'a>,
    form: Option<IndexTermForm>,
) -> Result<Vec<InlineNode<'a>>, &'static str> {
    if segment.text.is_empty() {
        return Ok(Vec::new());
    }

    // Each label newline becomes one space; equal byte lengths preserve source offsets.
    let text = if segment.text.contains('\n') {
        state.intern_str(&segment.text.replace('\n', " "))
    } else {
        segment.text
    };
    let end = segment.start + segment.text.len();
    let mut rules = state.inline_ctx.rules;
    rules.insert(InlineRules::AUTOLINKS);
    rules.remove(InlineRules::INDEX_TERMS | InlineRules::EOI_HARD_BREAK);
    if let Some(form) = form {
        rules.insert(InlineRules::INDEX_LABEL);
        rules.set(
            InlineRules::NAMED_INDEX_LABEL,
            matches!(
                form,
                IndexTermForm::NamedFlow | IndexTermForm::NamedConcealed
            ),
        );
    }
    let inline_ctx = InlineContext {
        offset: 0,
        substitutions: state.inline_ctx.substitutions,
        rules,
    };
    let mut child = ParserState::for_inline_parsing(text, state, inline_ctx);
    child.attribute_value_ranges = state
        .attribute_value_ranges
        .iter()
        .filter_map(|range| {
            let range_start = range.start.max(segment.start);
            let range_end = range.end.min(end);
            (range_start < range_end)
                .then(|| range_start - segment.start..range_end - segment.start)
        })
        .collect();
    child.empty_attribute_offsets = state
        .empty_attribute_offsets
        .iter()
        .filter(|offset| segment.start <= **offset && **offset <= end)
        .map(|offset| offset - segment.start)
        .collect();
    child.late_attribute_sources = state
        .late_attribute_sources
        .iter()
        .filter(|(range, _)| range.start >= segment.start && range.end <= end)
        .map(|(range, source)| {
            (
                range.start - segment.start..range.end - segment.start,
                *source,
            )
        })
        .collect();

    let parsed = inline_parser::inlines(text, &mut child)
        .map_err(|_| "could not parse index term content")?;

    let mut parsed = parsed;
    for inline in &mut parsed {
        crate::grammar::location_walk::walk_inline_locations_mut(inline, &mut |location| {
            location.absolute_start += segment.start;
            location.absolute_end += segment.start;
            location.start = state
                .line_map
                .offset_to_position(location.absolute_start, state.input);
            location.end = state
                .line_map
                .offset_to_position(location.absolute_end, state.input);
        });
    }
    Ok(parsed)
}

fn index_term_node(mut term: IndexTerm<'_>, plan: SubstitutionPlan) -> InlineNode<'_> {
    if term.catalog.as_ref().is_some_and(|catalog| {
        catalog.kind == term.kind && catalog.relationship == term.relationship
    }) && !plan.precedes(&Substitution::Macros, &Substitution::Replacements)
    {
        term.catalog = None;
    }
    if term.catalog.is_some() {
        term.catalog_substitutions = Some(plan);
    }
    InlineNode::Macro(InlineMacro::IndexTerm(Box::new(term)))
}

pub(super) fn expand_escaped_index_terms<'a>(
    nodes: Vec<InlineNode<'a>>,
    state: &mut ParserState<'a>,
) -> Result<Vec<InlineNode<'a>>, &'static str> {
    if !state.inline_ctx.rules.contains(InlineRules::INDEX_TERMS) || !state.input.contains('\\') {
        return Ok(nodes);
    }
    let mut expanded = Vec::with_capacity(nodes.len());
    for node in nodes {
        let InlineNode::PlainText(plain) = &node else {
            expanded.push(node);
            continue;
        };
        let tail = &state.input[plain.location.absolute_start..];
        let escapes = tail.len() - tail.trim_start_matches('\\').len();
        let index_literal = plain.content.starts_with("((")
            || plain.content.starts_with("indexterm:[")
            || plain.content.starts_with("indexterm2:[");
        if escapes == 0 || !index_literal {
            expanded.push(node);
            continue;
        }
        // Escaping the macro leaves its label available to other substitutions.
        let parsed = parse_index_term_inlines(
            state,
            IndexTermSegment {
                text: plain.content,
                start: plain.location.absolute_start + escapes,
            },
            None,
        )?;
        if matches!(parsed.as_slice(), [InlineNode::PlainText(text)] if text.content == plain.content)
        {
            expanded.push(node);
        } else {
            expanded.extend(parsed);
        }
    }
    Ok(expanded)
}

// Recognize a macro once, then use its registration-time parts for both catalog
// and display labels. Later attribute values cannot introduce separators.
pub(super) fn parse_index_term<'a>(
    state: &mut ParserState<'a>,
    content: IndexTermSegment<'a>,
    form: IndexTermForm,
    location: crate::Location,
) -> Result<InlineNode<'a>, &'static str> {
    let (source, restored_ranges) = registration_source(state, content);
    let mut parts = index_term_parts(state, source, form)?;
    let plan = state.inline_ctx.substitutions;
    let catalog = if plan.precedes(&Substitution::Macros, &Substitution::Attributes)
        || plan.precedes(&Substitution::Macros, &Substitution::Quotes)
        || plan.precedes(&Substitution::Macros, &Substitution::Replacements)
        || state.inline_ctx.rules.intersects(InlineRules::QUOTED_LINK)
        || (state.inline_ctx.rules.contains(InlineRules::BRACKET_LABEL) && source.contains("\\]"))
        || source
            .chars()
            .any(|character| matches!(character, ':' | '<' | '[' | '@'))
    {
        let mut inline_ctx = state.inline_ctx;
        inline_ctx.offset = 0;
        inline_ctx.substitutions = plan.through(&Substitution::Macros);
        inline_ctx.rules.insert(InlineRules::INDEX_CATALOG);
        // Index registration precedes enclosing-label bracket and quote unescaping.
        inline_ctx
            .rules
            .remove(InlineRules::BRACKET_LABEL | InlineRules::QUOTED_LINK);
        let mut child = ParserState::for_inline_parsing(source, state, inline_ctx);
        if restored_ranges.is_empty() {
            child.attribute_value_ranges = state
                .attribute_value_ranges
                .iter()
                .filter_map(|range| {
                    let start = range.start.max(content.start);
                    let end = range.end.min(content.start + content.text.len());
                    (start < end).then(|| start - content.start..end - content.start)
                })
                .collect();
        }
        let catalog_location = child.create_location(0, source.len().saturating_sub(1));
        let term = parse_index_parts(&mut child, parts.clone(), form, catalog_location)?;
        let mut node = InlineNode::Macro(InlineMacro::IndexTerm(Box::new(term)));
        map_registered_inline(&mut node, state, content.start, &restored_ranges);
        let InlineNode::Macro(InlineMacro::IndexTerm(mut term)) = node else {
            return Err("missing index catalog labels");
        };
        term.location = location.clone();
        Some(term)
    } else {
        None
    };
    let map_segment = |segment: &mut IndexTermSegment<'a>| {
        let start = content.start + restored_label_offset(segment.start, &restored_ranges, false);
        let end = content.start
            + restored_label_offset(segment.start + segment.text.len(), &restored_ranges, false);
        *segment = IndexTermSegment {
            text: &state.input[start..end],
            start,
        };
    };
    for segment in &mut parts.terms {
        map_segment(segment);
    }
    match &mut parts.relationship {
        Some(IndexTermRelationshipSegments::See(target)) => map_segment(target),
        Some(IndexTermRelationshipSegments::SeeAlso(targets)) => {
            for target in targets {
                map_segment(target);
            }
        }
        None => {}
    }
    let mut term = parse_index_parts(state, parts, form, location)?;
    term.catalog = catalog;
    Ok(index_term_node(term, plan))
}

fn index_term_parts<'a>(
    state: &mut ParserState<'a>,
    text: &'a str,
    form: IndexTermForm,
) -> Result<IndexTermParts<'a>, &'static str> {
    let content = trimmed_index_term_segment(text, 0);
    let (terms, relationship) = match form {
        IndexTermForm::Flow => {
            let (term, relationship) = split_index_term_relationship(content);
            (vec![term], relationship)
        }
        IndexTermForm::Concealed => {
            let (terms, relationship) = split_index_term_relationship(content);
            let terms = inline_parser::index_term_list(terms.text, state, terms.start)
                .map_err(|_| "could not parse index terms")?;
            (terms, relationship)
        }
        IndexTermForm::NamedFlow | IndexTermForm::NamedConcealed => {
            let items = inline_parser::index_term_macro_list(content.text, state, content.start)
                .map_err(|_| "could not parse index term attributes")?;
            let (terms, relationship) = partition_index_term_macro_items(items);
            let terms = if matches!(form, IndexTermForm::NamedFlow) && relationship.is_none() {
                vec![content]
            } else {
                terms
            };
            (terms, relationship)
        }
    };
    Ok(IndexTermParts {
        terms,
        relationship,
    })
}

fn parse_index_parts<'a>(
    state: &mut ParserState<'a>,
    parts: IndexTermParts<'a>,
    form: IndexTermForm,
    location: crate::Location,
) -> Result<IndexTerm<'a>, &'static str> {
    let mut terms = parts.terms.into_iter();
    let term = parse_index_term_inlines(
        state,
        terms
            .next()
            .unwrap_or(IndexTermSegment { text: "", start: 0 }),
        Some(form),
    )?;
    let kind = if form.visible() {
        IndexTermKind::Flow(term)
    } else {
        IndexTermKind::Concealed {
            term,
            secondary: terms
                .next()
                .map(|segment| parse_index_term_inlines(state, segment, Some(form)))
                .transpose()?,
            tertiary: terms
                .next()
                .map(|segment| parse_index_term_inlines(state, segment, Some(form)))
                .transpose()?,
        }
    };
    Ok(IndexTerm {
        kind,
        relationship: parse_index_term_relationship(state, parts.relationship, form)?,
        catalog: None,
        catalog_substitutions: None,
        location,
    })
}

fn parse_index_term_relationship<'a>(
    state: &mut ParserState<'a>,
    relationship: Option<IndexTermRelationshipSegments<'a>>,
    form: IndexTermForm,
) -> Result<Option<IndexTermRelationship<'a>>, &'static str> {
    match relationship {
        None => Ok(None),
        Some(IndexTermRelationshipSegments::See(target)) => Ok(Some(IndexTermRelationship::See {
            target: parse_index_term_inlines(state, target, Some(form))?,
        })),
        Some(IndexTermRelationshipSegments::SeeAlso(targets)) => {
            let targets = targets
                .into_iter()
                .filter(|target| !target.text.is_empty())
                .map(|target| parse_index_term_inlines(state, target, Some(form)))
                .collect::<Result<_, _>>()?;
            Ok(Some(IndexTermRelationship::SeeAlso { targets }))
        }
    }
}

fn partition_index_term_macro_items(
    items: Vec<IndexTermMacroItem<'_>>,
) -> (
    Vec<IndexTermSegment<'_>>,
    Option<IndexTermRelationshipSegments<'_>>,
) {
    let mut terms = Vec::new();
    let mut see = None;
    let mut see_also = None;
    for item in items {
        match item {
            IndexTermMacroItem::Term(term) => terms.push(term),
            IndexTermMacroItem::Relationship(IndexTermRelationshipSegments::See(target)) => {
                see = Some(IndexTermRelationshipSegments::See(target));
            }
            IndexTermMacroItem::Relationship(IndexTermRelationshipSegments::SeeAlso(targets)) => {
                see_also = Some(IndexTermRelationshipSegments::SeeAlso(targets));
            }
        }
    }
    (terms, see.or(see_also))
}
