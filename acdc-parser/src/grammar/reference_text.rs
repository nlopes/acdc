//! Reference-label interpretation and temporary inputs used by natural lookup.

use std::rc::Rc;

use super::{
    ParserState,
    attributes::{AttributeQuote, ResolvedContentAttribute, expand_attribute_list},
    inline_preprocessor::SourceMap,
    inline_processing::process_attribute_inlines_with_substitutions,
};
use crate::{
    CompatibilityMode, ContentAttribute, Error, InlineNode, Raw, Substitution,
    model::substitution::SubstitutionPlan,
};

#[derive(Debug)]
pub(super) struct ReferenceTextContext<'a> {
    pub(super) input: &'a str,
    pub(super) passthroughs: Rc<[crate::Pass<'a>]>,
}

#[derive(Clone, Debug)]
pub(super) struct ReferenceTextSource<'a> {
    value: ResolvedContentAttribute<'a>,
    substitutions: Option<SubstitutionPlan>,
    content: Option<ContentAttribute<'a>>,
}

fn capture<'a>(
    state: &ParserState<'a>,
    value: ResolvedContentAttribute<'a>,
    substitutions: Option<SubstitutionPlan>,
) -> usize {
    let mut sources = state.reference_text_sources.borrow_mut();
    let id = sources.len();
    sources.push(ReferenceTextSource {
        value,
        substitutions,
        content: None,
    });
    id
}

pub(super) fn capture_named<'a>(
    state: &ParserState<'a>,
    value: ResolvedContentAttribute<'a>,
) -> usize {
    if state.options.compatibility_mode == CompatibilityMode::Draft
        && value.quote != AttributeQuote::Single
    {
        capture(state, value, None)
    } else {
        capture_block(state, value)
    }
}

pub(super) fn capture_block<'a>(
    state: &ParserState<'a>,
    value: ResolvedContentAttribute<'a>,
) -> usize {
    let substitutions = match state.options.compatibility_mode {
        CompatibilityMode::Asciidoctor => SubstitutionPlan::only(&Substitution::Quotes),
        CompatibilityMode::Draft => SubstitutionPlan::default().without(&Substitution::Attributes),
    };
    capture(state, value, Some(substitutions))
}

pub(super) fn capture_inline<'a>(
    state: &ParserState<'a>,
    text: &'a str,
    start: usize,
    expand: bool,
) -> usize {
    let source_end = start + text.len();
    let (text, map) = if expand {
        let expanded = expand_attribute_list(
            text,
            &state.document_attributes,
            state.options.compatibility_mode,
        );
        (state.intern_cow(expanded.text), expanded.source_map)
    } else {
        (text, SourceMap::default())
    };
    capture(
        state,
        restore_passthroughs(
            state,
            ResolvedContentAttribute {
                text,
                quote: AttributeQuote::Unquoted,
                source_map: map.with_origin(SourceMap::default(), start),
                source_range: start..source_end,
                attributes: Rc::clone(&state.document_attributes),
            },
        ),
        Some(SubstitutionPlan::only(&Substitution::Quotes)),
    )
}

pub(super) fn text<'a>(state: &ParserState<'a>, id: Option<usize>) -> Option<&'a str> {
    id.and_then(|id| {
        state
            .reference_text_sources
            .borrow()
            .get(id)
            .map(|source| source.value.text)
    })
}

fn missing_source(state: &ParserState<'_>) -> Error {
    Error::Parse(
        Box::new(state.create_error_source_location(state.create_location(0, 0))),
        "missing reference-label parsing input".into(),
    )
}

fn source<'a>(state: &ParserState<'a>, id: usize) -> Result<ReferenceTextSource<'a>, Error> {
    state
        .reference_text_sources
        .borrow()
        .get(id)
        .cloned()
        .ok_or_else(|| missing_source(state))
}

pub(super) fn resolve<'a>(
    state: &ParserState<'a>,
    id: usize,
) -> Result<ContentAttribute<'a>, Error> {
    let source = source(state, id)?;
    if let Some(content) = source.content {
        return Ok(content);
    }
    let value = &source.value;
    let nodes = if value.text.is_empty() {
        Vec::new()
    } else if let Some(substitutions) = source.substitutions {
        process_attribute_inlines_with_substitutions(state, value, substitutions)?
    } else {
        vec![InlineNode::RawText(Raw {
            content: value.text,
            location: state.create_location(
                value.source_map.map_position(0)?,
                value.source_map.map_end_position(value.text.len() - 1)?,
            ),
            subs: vec![Substitution::SpecialChars],
        })]
    };
    let content = ContentAttribute::new(nodes);
    state
        .reference_text_sources
        .borrow_mut()
        .get_mut(id)
        .ok_or_else(|| missing_source(state))?
        .content = Some(content.clone());
    Ok(content)
}

pub(super) fn capture_macro<'a>(state: &ParserState<'a>, text: &'a str, start: usize) -> usize {
    let mut map = SourceMap::default();
    for (offset, _) in text.match_indices("\\]") {
        map.add_replacement(
            offset,
            offset + 2,
            1,
            super::inline_preprocessor::ProcessedKind::Escape,
        );
    }
    let decoded = state.intern_str(&text.replace("\\]", "]"));
    capture(
        state,
        restore_passthroughs(
            state,
            ResolvedContentAttribute {
                text: decoded,
                quote: AttributeQuote::Unquoted,
                source_map: map.with_origin(SourceMap::default(), start),
                source_range: start..start + text.len(),
                attributes: Rc::clone(&state.document_attributes),
            },
        ),
        Some(SubstitutionPlan::only(&Substitution::Quotes)),
    )
}

fn restore_passthroughs<'a>(
    state: &ParserState<'a>,
    mut value: ResolvedContentAttribute<'a>,
) -> ResolvedContentAttribute<'a> {
    use super::inline_preprocessor::ProcessedKind;
    let Some(context) = &state.reference_text_context else {
        return value;
    };
    let mut replacements = Vec::new();
    for (index, passthrough) in context.passthroughs.iter().enumerate() {
        let placeholder = format!("���{index}���");
        let replacement = if passthrough.kind == crate::PassthroughKind::AttributeRef {
            passthrough.text.unwrap_or_default()
        } else {
            context
                .input
                .get(passthrough.location.absolute_start..passthrough.location.absolute_end)
                .or(passthrough.text)
                .unwrap_or_default()
        };
        for (start, _) in value.text.match_indices(&placeholder) {
            replacements.push((start, start + placeholder.len(), replacement));
        }
    }
    if !replacements.is_empty() {
        replacements.sort_by_key(|(start, _, _)| *start);
        let mut restored = String::new();
        let mut map = SourceMap::default();
        let mut cursor = 0;
        for (start, end, replacement) in replacements {
            restored.push_str(&value.text[cursor..start]);
            restored.push_str(replacement);
            map.add_replacement(start, end, replacement.len(), ProcessedKind::Attribute);
            cursor = end;
        }
        restored.push_str(&value.text[cursor..]);
        value.text = state.intern_str(&restored);
        value.source_map = map.with_origin(value.source_map, 0);
    }
    value
}

pub(super) fn resolve_tree<'a>(
    state: &mut ParserState<'a>,
    header: &mut Option<crate::Header<'a>>,
    blocks: &mut [crate::Block<'a>],
) -> Result<(), Error> {
    super::location_walk::walk_reference_texts_mut(header, blocks, &mut |content, source| {
        if content.is_none()
            && let Some(id) = *source
        {
            *content = Some(resolve(state, id)?);
        }
        Ok(())
    })
}

pub(crate) fn resolve_inline_tree<'a>(
    state: &mut ParserState<'a>,
    inlines: &mut [InlineNode<'a>],
) -> Result<(), Error> {
    super::location_walk::walk_inline_reference_texts_mut(inlines, &mut |content, source| {
        if let Some(id) = source.take()
            && content.is_none()
        {
            *content = Some(resolve(state, id)?);
        }
        Ok(())
    })
}

pub(crate) fn finish_document<'a>(
    state: &mut ParserState<'a>,
    document: &mut crate::Document<'a>,
) -> Result<(), Error> {
    super::location_walk::walk_document_reference_texts_mut(document, &mut |content, source| {
        if let Some(id) = source.take()
            && content.is_none()
        {
            *content = Some(resolve(state, id)?);
        }
        Ok(())
    })
}

pub(super) fn resolve_metadata<'a>(
    state: &mut ParserState<'a>,
    metadata: &mut crate::BlockMetadata<'a>,
) -> Result<(), Error> {
    let selected = metadata.reftext_source.or_else(|| {
        metadata
            .anchors
            .last()
            .and_then(|anchor| anchor.reftext_source)
    });
    for anchor in &mut metadata.anchors {
        if anchor.reftext_source != selected {
            anchor.reftext_source = None;
            anchor.xreflabel = None;
        }
    }
    if let Some(id) = selected {
        let content = resolve(state, id)?;
        if metadata.reftext_source.is_some() {
            metadata.reftext = Some(content);
        } else if let Some(anchor) = metadata.anchors.last_mut() {
            anchor.xreflabel = Some(content);
        }
    }
    Ok(())
}

pub(super) fn refresh_block_anchor(state: &ParserState<'_>, id: usize) -> Result<usize, Error> {
    let source = source(state, id)?;
    let range = source.value.source_range;
    let authored = state.input.get(range.clone()).ok_or_else(|| {
        Error::Parse(
            Box::new(
                state.create_error_source_location(state.create_location(range.start, range.end)),
            ),
            "invalid block anchor reference-text range".into(),
        )
    })?;
    let (text, map) = super::attributes::resolve_block_anchor_reftext(
        authored,
        &state.document_attributes,
        state.options.compatibility_mode,
    )
    .ok_or_else(|| {
        Error::Parse(
            Box::new(
                state.create_error_source_location(state.create_location(range.start, range.end)),
            ),
            "invalid block anchor reference text after attribute expansion".into(),
        )
    })?;
    Ok(capture_block(
        state,
        ResolvedContentAttribute {
            text: state.intern_cow(text),
            quote: AttributeQuote::Unquoted,
            source_map: map.with_origin(SourceMap::default(), range.start),
            source_range: range,
            attributes: Rc::clone(&state.document_attributes),
        },
    ))
}
