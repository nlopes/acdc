//! Index terms and their reference labels.

use std::borrow::Cow;

use crate::{
    CompatibilityMode, IndexTerm, IndexTermKind, IndexTermRelationship, InlineMacro, InlineNode,
    Substitution,
    grammar::{
        ParserState,
        attributes::{AttributeQuote, ScannedAttribute, scan_attribute_list},
        inline_parser,
        inline_preprocessor::{ProcessedKind, SourceMap},
        inlines::{
            attributes::insert_named_metadata,
            recognition::structural_token_allowed,
            text::{map_registered_inline, registration_source, restored_label_offset},
        },
        state::{InlineContext, InlineRules},
    },
    model::substitution::SubstitutionPlan,
};

#[derive(Clone, Copy)]
pub(super) struct IndexTermSegment<'a> {
    pub(super) text: &'a str,
    pub(super) start: usize,
}

#[derive(Clone, Copy)]
pub(super) struct IndexTermLabel<'a> {
    pub(super) text: &'a str,
    pub(super) start: usize,
    quote: Option<char>,
}

impl<'a> From<IndexTermSegment<'a>> for IndexTermLabel<'a> {
    fn from(segment: IndexTermSegment<'a>) -> Self {
        Self {
            text: segment.text,
            start: segment.start,
            quote: None,
        }
    }
}

#[derive(Clone)]
pub(super) enum IndexTermRelationshipSegments<'a> {
    See(IndexTermLabel<'a>),
    SeeAlso(Vec<IndexTermLabel<'a>>),
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
    terms: Vec<IndexTermLabel<'a>>,
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
                trimmed_index_term_segment(target, segment.start + term.len() + 4).into(),
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
                segment.into()
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
    segment: IndexTermLabel<'a>,
    form: Option<IndexTermForm>,
) -> Result<Vec<InlineNode<'a>>, &'static str> {
    if segment.text.is_empty() {
        return Ok(Vec::new());
    }

    let mut source_map = SourceMap::default();
    if let Some(quote) = segment.quote {
        for (offset, _) in segment.text.match_indices(&format!("\\{quote}")) {
            if structural_token_allowed(state, &Substitution::Macros, segment.start + offset, 2) {
                source_map.add_replacement(offset, offset + 2, 1, ProcessedKind::Escape);
            }
        }
    }
    let text = if source_map.replacements.is_empty() && !segment.text.contains('\n') {
        segment.text
    } else {
        let mut decoded = String::with_capacity(segment.text.len());
        let mut cursor = 0;
        for replacement in &source_map.replacements {
            decoded.push_str(&segment.text[cursor..replacement.absolute_start]);
            cursor = replacement.absolute_start + 1;
        }
        decoded.push_str(&segment.text[cursor..]);
        state.intern_str(&decoded.replace('\n', " "))
    };
    let unescaped_offset = |position: usize| {
        position
            - source_map
                .replacements
                .partition_point(|replacement| replacement.absolute_start < position)
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
            (range_start < range_end).then(|| {
                unescaped_offset(range_start - segment.start)
                    ..unescaped_offset(range_end - segment.start)
            })
        })
        .collect();
    child.empty_attribute_offsets = state
        .empty_attribute_offsets
        .iter()
        .filter(|offset| segment.start <= **offset && **offset <= end)
        .map(|offset| unescaped_offset(offset - segment.start))
        .collect();
    child.late_attribute_sources = state
        .late_attribute_sources
        .iter()
        .filter(|(range, _)| range.start >= segment.start && range.end <= end)
        .map(|(range, source)| {
            (
                unescaped_offset(range.start - segment.start)
                    ..unescaped_offset(range.end - segment.start),
                *source,
            )
        })
        .collect();

    let parsed = inline_parser::inlines(text, &mut child)
        .map_err(|_| "could not parse index term content")?;

    let mut parsed = parsed;
    for inline in &mut parsed {
        crate::grammar::location_walk::try_walk_inline_locations_mut(inline, &mut |location| {
            let start = segment.start + source_map.map_position(location.absolute_start)?;
            let end = segment.start + source_map.map_end_position(location.absolute_end)?;
            *location = state.create_location(start, end);
            Ok::<_, crate::Error>(())
        })
        .map_err(|_| "could not map index label content")?;
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
            }
            .into(),
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
    let (mut parts, attributes) = index_term_parts(state, source, form)?;
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
    let map_segment = |segment: &mut IndexTermLabel<'a>| {
        let start = content.start + restored_label_offset(segment.start, &restored_ranges, false);
        let end = content.start
            + restored_label_offset(segment.start + segment.text.len(), &restored_ranges, false);
        segment.text = &state.input[start..end];
        segment.start = start;
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
    for attribute in attributes {
        let Some(name) = attribute.name else {
            continue;
        };
        let name = match name.as_str() {
            "roles" => Cow::Borrowed("role"),
            "opts" => Cow::Borrowed("options"),
            _ => Cow::Owned(name),
        };
        let start =
            content.start + restored_label_offset(attribute.value_start, &restored_ranges, false);
        let end =
            content.start + restored_label_offset(attribute.value_end, &restored_ranges, false);
        insert_named_metadata(
            state,
            &mut term.metadata,
            &mut term.macro_attributes,
            name,
            attribute.value,
            start..end,
        );
    }
    term.catalog = catalog;
    Ok(index_term_node(term, plan))
}

fn index_term_parts<'a>(
    state: &mut ParserState<'a>,
    text: &'a str,
    form: IndexTermForm,
) -> Result<(IndexTermParts<'a>, Vec<ScannedAttribute>), &'static str> {
    let content = trimmed_index_term_segment(text, 0);
    let (terms, relationship) = match form {
        IndexTermForm::Flow => {
            let (term, relationship) = split_index_term_relationship(content);
            (vec![term.into()], relationship)
        }
        IndexTermForm::Concealed => {
            let (terms, relationship) = split_index_term_relationship(content);
            let terms = inline_parser::index_term_list(terms.text, state, terms.start)
                .map_err(|_| "could not parse index terms")?;
            (terms.into_iter().map(Into::into).collect(), relationship)
        }
        IndexTermForm::NamedFlow | IndexTermForm::NamedConcealed => {
            if text.contains('=') {
                return named_index_parts(state, text, form);
            }
            let terms = if form.visible() {
                vec![content.into()]
            } else {
                inline_parser::named_index_term_list(content.text, state, content.start)
                    .map_err(|_| "could not parse index terms")?
                    .into_iter()
                    .map(Into::into)
                    .collect()
            };
            (terms, None)
        }
    };
    Ok((
        IndexTermParts {
            terms,
            relationship,
        },
        Vec::new(),
    ))
}

fn attribute_ranges<'a>(
    state: &mut ParserState<'a>,
    text: &'a str,
) -> Result<Vec<std::ops::Range<usize>>, &'static str> {
    let mut context = state.inline_ctx;
    context.offset = 0;
    let mut child = ParserState::for_inline_parsing(text, state, context);
    inline_parser::index_term_attribute_ranges(text, &mut child)
        .map_err(|_| "could not scan nested index content")
}

fn named_index_parts<'a>(
    state: &mut ParserState<'a>,
    text: &'a str,
    form: IndexTermForm,
) -> Result<(IndexTermParts<'a>, Vec<ScannedAttribute>), &'static str> {
    let protected = attribute_ranges(state, text)?;
    let scanned = scan_attribute_list(text, &protected);
    if scanned.first().is_none_or(|attribute| {
        attribute.name.is_some()
            || (attribute.value.is_empty() && attribute.quote == AttributeQuote::Unquoted)
    }) {
        return Ok((
            IndexTermParts {
                terms: vec![trimmed_index_term_segment(text, 0).into()],
                relationship: None,
            },
            Vec::new(),
        ));
    }
    let mut terms = Vec::new();
    let mut controls = Vec::new();
    let mut see = None;
    let mut see_also = None;
    let limit = if form.visible() { 1 } else { 3 };
    for (slot, attribute) in scanned.into_iter().enumerate() {
        if attribute.is_ignored(CompatibilityMode::Asciidoctor) {
            continue;
        }
        let quote = match attribute.quote {
            AttributeQuote::Double => Some('"'),
            AttributeQuote::Single => Some('\''),
            AttributeQuote::Unquoted => None,
        };
        let label = IndexTermLabel {
            text: &text[attribute.value_start..attribute.value_end],
            start: attribute.value_start,
            quote,
        };
        match attribute.name.as_deref() {
            None if slot < limit
                && slot == terms.len()
                && (!label.text.is_empty() || quote.is_some()) =>
            {
                terms.push(label);
            }
            None | Some("terms") => {}
            Some("see") => see = Some(IndexTermRelationshipSegments::See(label)),
            Some("see-also") => {
                let nested = attribute_ranges(state, label.text)?;
                let mut targets = Vec::new();
                let mut start = 0;
                for (end, _) in label
                    .text
                    .match_indices(',')
                    .filter(|(offset, _)| !nested.iter().any(|range| range.contains(offset)))
                {
                    let part =
                        trimmed_index_term_segment(&label.text[start..end], label.start + start);
                    targets.push(IndexTermLabel {
                        text: part.text,
                        start: part.start,
                        quote,
                    });
                    start = end + 1;
                }
                let part = trimmed_index_term_segment(&label.text[start..], label.start + start);
                targets.push(IndexTermLabel {
                    text: part.text,
                    start: part.start,
                    quote,
                });
                see_also = Some(IndexTermRelationshipSegments::SeeAlso(targets));
            }
            Some(_) => controls.push(attribute),
        }
    }
    Ok((
        IndexTermParts {
            terms,
            relationship: see.or(see_also),
        },
        controls,
    ))
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
            .unwrap_or_else(|| IndexTermSegment { text: "", start: 0 }.into()),
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
    let mut term = IndexTerm::new(kind, location);
    term.relationship = parse_index_term_relationship(state, parts.relationship, form)?;
    Ok(term)
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
