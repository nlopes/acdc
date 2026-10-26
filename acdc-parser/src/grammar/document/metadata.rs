//! Block styles, attributes, anchors, and captions.

use crate::{
    Anchor, AttributeValue, Attribution, Block, BlockMetadata, CiteTitle, CompatibilityMode,
    ContentAttribute, Error, InlineNode, Location, Plain, Title, Warning, WarningKind,
    document_attribute::AttributeDeclaration,
    grammar::{
        ParserState,
        attributes::{
            AttributeQuote, BlockAttributeMode, ResolvedContentAttribute, ScannedAttribute,
            ShorthandKind, block_attribute_entries, block_attribute_line,
            block_attribute_list_allowed, block_list_style, block_positional_style,
            block_shorthand, decode_anchor_shorthand_reftext, expand_attribute_list,
            scan_block_attribute_list,
        },
        helpers::{
            BlockParsingMetadata, RESERVED_NAMED_ATTRIBUTE_ID, RESERVED_NAMED_ATTRIBUTE_OPTIONS,
            RESERVED_NAMED_ATTRIBUTE_ROLE, RESERVED_NAMED_ATTRIBUTE_SUBS,
        },
        inline_preprocessor::SourceMap,
        inline_processing::{process_attribute_inlines, process_inlines},
    },
    model::{
        AttributePosition, Caption, CaptionKind, PositionalAttribute, SectionLevel, Substitution,
        substitute,
        substitution::{SubstitutionPlan, parse_subs_attribute},
    },
};
use std::{borrow::Cow, rc::Rc};

use super::delimited::DelimitedKind;

#[derive(Clone, Copy, Debug)]
pub(super) enum AttributeBlockForm<'a> {
    Delimited(DelimitedKind, Option<&'a str>),
    Paragraph { indented: bool },
    NonParagraph,
    Table,
}

/// A block metadata line recognized by the document grammar.
#[derive(Debug)]
pub(super) enum BlockMetadataLine<'input> {
    Anchor(Anchor<'input>),
    Attributes(RawAttributeList<'input>),
    Title {
        source: &'input str,
        start: usize,
        end: usize,
    },
    DocumentAttribute(AttributeDeclaration<'input>, Location),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RawAttributeList<'a> {
    pub(super) source: &'a str,
    pub(super) start: usize,
}

#[derive(Clone, Debug)]
pub(super) enum TitleSource<'a> {
    Dot {
        source: &'a str,
        start: usize,
        end: usize,
    },
    Attribute(ResolvedContentAttribute<'a>),
}

pub(super) fn resolve_title<'a>(
    state: &mut ParserState<'a>,
    source: TitleSource<'a>,
    offset: usize,
) -> Result<Title<'a>, Error> {
    let inlines = match source {
        TitleSource::Dot { source, start, end } => {
            process_inlines(
                state,
                &BlockParsingMetadata::default(),
                start,
                end,
                offset,
                source,
            )?
            .0
        }
        TitleSource::Attribute(source)
            if state.options.compatibility_mode == CompatibilityMode::Draft
                && source.quote != AttributeQuote::Single =>
        {
            if source.text.is_empty() {
                Vec::new()
            } else {
                vec![InlineNode::RawText(crate::Raw {
                    content: source.text,
                    location: state.create_block_location(
                        source.source_range.start,
                        source.source_range.end,
                        0,
                    ),
                    subs: vec![Substitution::SpecialChars],
                })]
            }
        }
        TitleSource::Attribute(source) => process_attribute_inlines(state, &source)?,
    };
    Ok(Title::new(inlines))
}

fn unique(values: &mut Vec<&str>) {
    let mut seen = std::collections::HashSet::new();
    values.retain(|value| seen.insert(*value));
}

/// Metadata accepted where title and document-attribute lines are not allowed.
#[derive(Debug)]
pub(super) enum AttributeOrAnchorLine<'input> {
    Anchor(Anchor<'input>),
    Attributes((bool, Box<BlockMetadata<'input>>)),
}

/// Resolve the block caption after parsing its content. An attribute inside an
/// example can change that example's caption, as in asciidoctor. Caption numbers
/// are assigned after the document is complete.
pub(super) fn assign_block_caption<'input>(state: &ParserState<'input>, block: &mut Block<'input>) {
    if let Some(metadata) = block.metadata_mut() {
        metadata.alt_source = None;
        metadata.caption_source = None;
    }
    let Some(kind) = CaptionKind::for_block(block) else {
        return;
    };
    if let Some(metadata) = block.metadata_mut() {
        let caption = Caption::resolve(metadata, &state.document_attributes, kind);
        metadata.caption = Some(caption);
    }
}

pub(super) fn order_document_attribute_events(blocks: Vec<Block<'_>>) -> Vec<Block<'_>> {
    let needs_ordering = blocks.iter().any(|block| {
        if let Block::DocumentAttribute(attribute) = block {
            return !attribute.is_accepted();
        }
        block
            .metadata()
            .is_some_and(BlockMetadata::has_document_attributes)
    });
    if !needs_ordering {
        return blocks;
    }

    let mut ordered = Vec::with_capacity(blocks.len());
    for mut block in blocks {
        if matches!(&block, Block::DocumentAttribute(attribute) if !attribute.is_accepted()) {
            continue;
        }
        if let Some(metadata) = block.metadata_mut() {
            ordered.extend(
                metadata
                    .take_document_attributes()
                    .into_iter()
                    .map(Block::DocumentAttribute),
            );
        }
        ordered.push(block);
    }
    ordered
}

pub(super) fn push_metadata_anchor<'a>(metadata: &mut BlockMetadata<'a>, anchor: Anchor<'a>) {
    // A later anchor label replaces an earlier named reftext; an ID alone does not.
    if let Some(label) = anchor.xreflabel
        && metadata.attributes.contains_key("reftext")
    {
        metadata.attributes.set(
            "reftext".into(),
            AttributeValue::String(Cow::Borrowed(label)),
        );
    }
    metadata.anchors.push(anchor);
}

pub(super) fn merge_attribute_metadata<'input>(
    metadata: &mut BlockMetadata<'input>,
    mut attribute_metadata: BlockMetadata<'input>,
    mode: CompatibilityMode,
) {
    metadata.overlay_positional_attributes(&attribute_metadata, mode);
    metadata.append_document_attributes(attribute_metadata.take_document_attributes());
    if mode == CompatibilityMode::Asciidoctor
        && attribute_metadata
            .attribute_positions
            .as_ref()
            .is_some_and(|positions| positions.named.contains_key("role"))
    {
        metadata.roles.clear();
    }
    if let Some(positions) = attribute_metadata.attribute_positions.take() {
        let target = metadata
            .attribute_positions
            .get_or_insert_with(Default::default);
        if positions.first.is_some() {
            target.first = positions.first;
        }
        target.named.extend(positions.named);
    }
    metadata.anchors.extend(attribute_metadata.anchors);
    if attribute_metadata.id.is_some() {
        metadata.id = attribute_metadata.id;
    }
    if attribute_metadata.style.is_some() {
        metadata.style = attribute_metadata.style;
    }
    metadata.roles.extend(attribute_metadata.roles);
    metadata.options.extend(attribute_metadata.options);
    if mode == CompatibilityMode::Draft {
        unique(&mut metadata.roles);
        unique(&mut metadata.options);
    }
    for (name, value) in attribute_metadata.attributes.iter() {
        metadata.attributes.set(name.clone(), value.clone());
    }
    if attribute_metadata.first_source.is_some() {
        metadata.first_source = attribute_metadata.first_source;
    }
    if attribute_metadata.alt_source.is_some() {
        metadata.alt_source = attribute_metadata.alt_source;
    }
    if attribute_metadata.caption_source.is_some() {
        metadata.caption_source = attribute_metadata.caption_source;
    }
    if attribute_metadata.caption.is_some() {
        metadata.caption = attribute_metadata.caption;
    }
    if attribute_metadata.substitutions.is_some() {
        metadata.substitutions = attribute_metadata.substitutions;
    }
    if attribute_metadata.attribution.is_some() {
        metadata.attribution = attribute_metadata.attribution;
        metadata.attribution_substitutions = attribute_metadata.attribution_substitutions;
    } else if attribute_metadata.attribution_substitutions {
        metadata.attribution_substitutions = true;
    }
    if attribute_metadata.citetitle.is_some() {
        metadata.citetitle = attribute_metadata.citetitle;
        metadata.citetitle_substitutions = attribute_metadata.citetitle_substitutions;
    } else if attribute_metadata.citetitle_substitutions {
        metadata.citetitle_substitutions = true;
    }
}

pub(super) fn merge_macro_metadata<'a>(
    metadata: &mut BlockMetadata<'a>,
    attributes: &BlockMetadata<'a>,
    mode: CompatibilityMode,
) {
    merge_attribute_metadata(metadata, attributes.clone(), mode);
}

pub(super) fn media_attributes_ahead<'input>(
    state: &ParserState<'input>,
    line_start: usize,
    offset: usize,
) -> Option<RawAttributeList<'input>> {
    let line = state.input.get(line_start + offset..)?.lines().next()?;
    let start = line.find('[')? + 1;
    let end = line.rfind(']')?;
    let source = line.get(start..end)?;
    if !block_attribute_list_allowed(
        source,
        &state.document_attributes,
        state.options.compatibility_mode,
        BlockAttributeMode::Macro,
    ) {
        return None;
    }
    Some(RawAttributeList {
        source,
        start: line_start + start,
    })
}

pub(super) fn finish_block_parsing_metadata<'input>(
    state: &mut ParserState<'input>,
    mut metadata: BlockMetadata<'input>,
    title: Title<'input>,
    parent_section_level: Option<SectionLevel>,
    offset: usize,
) -> Result<BlockParsingMetadata<'input>, Error> {
    let discrete = matches!(metadata.style, Some("discrete" | "float"));
    let substitutions = metadata
        .substitutions
        .as_ref()
        .map_or_else(SubstitutionPlan::default, SubstitutionPlan::for_block_spec);
    let hardbreaks = substitutions.enabled(&Substitution::PostReplacements)
        && (state.hardbreaks || metadata.options.contains(&"hardbreaks"));
    resolve_caption_content(state, &mut metadata, offset)?;
    apply_quote_attribute_substitutions(state, &mut metadata, offset, substitutions)?;
    Ok(BlockParsingMetadata {
        metadata,
        title,
        parent_section_level,
        substitutions,
        hardbreaks,
        discrete,
        media_attributes: None,
        image_alt: None,
    })
}

pub(super) fn select_heading_style<'a>(
    positional: Option<&'a str>,
    selected: Option<&'a str>,
    mode: CompatibilityMode,
) -> Option<&'a str> {
    // Asciidoctor uses the first positional value to classify headings; named
    // style attributes do not change section boundaries or discrete headings.
    match mode {
        CompatibilityMode::Asciidoctor => positional,
        CompatibilityMode::Draft => selected,
    }
}

// Boundary lookahead must not parse titles or register their inline content.
pub(super) fn metadata_marks_discrete_heading(metadata: &str, state: &ParserState<'_>) -> bool {
    let mut discrete = false;
    for line in metadata.lines() {
        let Some(attributes) = block_attribute_line(
            line,
            &state.document_attributes,
            state.options.compatibility_mode,
        ) else {
            continue;
        };
        if attributes.starts_with('[')
            && (state.options.compatibility_mode == CompatibilityMode::Asciidoctor
                || attributes.starts_with("[["))
        {
            continue;
        }
        let attributes = expand_attribute_list(
            attributes,
            &state.document_attributes,
            state.options.compatibility_mode,
        );
        let attributes = scan_block_attribute_list(
            &attributes.text,
            state.options.compatibility_mode,
            BlockAttributeMode::Block,
        );
        let mode = state.options.compatibility_mode;
        let positional = attributes
            .first()
            .and_then(|attribute| block_positional_style(attribute, mode));
        if let Some(style) =
            select_heading_style(positional, block_list_style(&attributes, mode), mode)
        {
            discrete = matches!(style, "discrete" | "float");
        }
    }
    discrete
}

fn apply_block_style<'input>(
    state: &ParserState<'input>,
    metadata: &mut BlockMetadata<'input>,
    attribute: &ScannedAttribute,
    source_map: &SourceMap,
    content_start: usize,
    offset: usize,
) -> Result<(), Error> {
    let value = state.intern_str(&attribute.value);
    for part in block_shorthand(value, attribute.quote, state.options.compatibility_mode) {
        let text = &value[part.range.clone()];
        match part.kind {
            ShorthandKind::Style => metadata.style = Some(text),
            ShorthandKind::Id => {
                let range = attribute.source_range(source_map, part.range)?;
                metadata.id = Some(Anchor {
                    id: text,
                    xreflabel: None,
                    location: state.create_block_location(
                        content_start + range.start,
                        content_start + range.end,
                        offset,
                    ),
                    bibliography_label: None,
                    bibliography: false,
                });
            }
            ShorthandKind::Role => metadata.roles.push(text),
            ShorthandKind::Option => metadata.options.push(text),
            ShorthandKind::Reftext => {
                metadata.attributes.set(
                    "reftext".into(),
                    state
                        .intern_str(&decode_anchor_shorthand_reftext(text))
                        .into(),
                );
            }
        }
    }
    Ok(())
}

fn positional_wins(
    metadata: &BlockMetadata<'_>,
    position: Option<AttributePosition>,
    name: &str,
    mode: CompatibilityMode,
    media: bool,
) -> bool {
    // Asciidoctor rekeys ordinary blocks after named entries, but assigns macro
    // fields in source order. Draft uses source order for both.
    if mode == CompatibilityMode::Asciidoctor && !media {
        return true;
    }
    metadata
        .attribute_positions
        .as_ref()
        .and_then(|positions| positions.named.get(name))
        .is_none_or(|named| position.is_none_or(|position| position > *named))
}

pub(super) fn route_media_attributes<'a>(
    metadata: &mut BlockMetadata<'a>,
    attributes: &BlockMetadata<'a>,
    mode: CompatibilityMode,
    video: bool,
) {
    merge_macro_metadata(metadata, attributes, mode);
    let selected = if mode == CompatibilityMode::Draft {
        &*metadata
    } else {
        attributes
    };
    let first = selected.first_source.clone();
    let first_position = selected
        .attribute_positions
        .as_ref()
        .and_then(|positions| positions.first);
    for (slot, name) in ["width", "height"].into_iter().enumerate() {
        let aggregated = metadata.take_next_positional_attribute();
        let attribute = if mode == CompatibilityMode::Draft {
            aggregated
        } else {
            attributes.positional_attributes.get(slot).cloned()
        };
        if let Some(attribute) = attribute.filter(|attribute| {
            !attribute.value.is_empty()
                || (mode == CompatibilityMode::Asciidoctor && attribute.quoted_empty)
        }) && positional_wins(metadata, attribute.position, name, mode, true)
        {
            metadata.attributes.set(name.into(), attribute.value.into());
        }
    }
    if video
        && let Some(first) = first
        && positional_wins(metadata, first_position, "poster", mode, true)
    {
        metadata
            .attributes
            .set("poster".into(), first.source.into());
    }
    metadata.style = None;
    metadata.first_source = None;
}

pub(super) fn route_block_positions<'a>(
    state: &ParserState<'a>,
    metadata: &mut BlockMetadata<'a>,
    form: AttributeBlockForm<'a>,
) {
    let (source, quote) = match form {
        AttributeBlockForm::Delimited(DelimitedKind::Listing, language) => {
            if let Some(language) = language {
                // A fence supplies its language without rekeying preceding positions.
                metadata.style = Some("source");
                extract_source_attributes(state, metadata, Some(language));
                return;
            } else if metadata.style.is_none() && metadata.positional_attribute(0).is_some() {
                metadata.style = Some("source");
            }
            (metadata.style == Some("source"), false)
        }
        AttributeBlockForm::Delimited(DelimitedKind::Quote, _) => (false, true),
        AttributeBlockForm::Delimited(DelimitedKind::Open, _)
        | AttributeBlockForm::Paragraph { indented: false } => (
            metadata.style == Some("source"),
            matches!(metadata.style, Some("quote" | "verse")),
        ),
        AttributeBlockForm::Paragraph { indented: true } | AttributeBlockForm::NonParagraph => (
            metadata.style == Some("source"),
            metadata.style == Some("verse"),
        ),
        AttributeBlockForm::Delimited(_, _) | AttributeBlockForm::Table => (false, false),
    };
    if source {
        extract_source_attributes(state, metadata, None);
    }
    if quote {
        extract_quote_attributes(metadata, state.options.compatibility_mode);
    }
}

fn ensure_positional_slot(metadata: &mut BlockMetadata<'_>, slot: usize) {
    metadata
        .positional_attributes
        .resize(slot, PositionalAttribute::default());
}

fn extract_source_attributes<'a>(
    state: &ParserState<'a>,
    metadata: &mut BlockMetadata<'a>,
    fence_language: Option<&'a str>,
) {
    if metadata.style != Some("source") {
        return;
    }

    if let Some(language) = fence_language {
        metadata.attributes.set("language".into(), language.into());
    } else {
        for (slot, name) in [(0, "language"), (1, "linenums")] {
            if let Some(attribute) = metadata
                .positional_attributes
                .get(slot)
                .filter(|attribute| {
                    !attribute.value.is_empty()
                        || (state.options.compatibility_mode == CompatibilityMode::Asciidoctor
                            && attribute.quoted_empty)
                })
                && positional_wins(
                    metadata,
                    attribute.position,
                    name,
                    state.options.compatibility_mode,
                    false,
                )
            {
                metadata.attributes.set(name.into(), attribute.value.into());
            }
        }
    }
    if !metadata.attributes.contains_key("linenums")
        && (metadata.options.contains(&"linenums")
            || state
                .document_attributes
                .contains_key("source-linenums-option"))
    {
        metadata
            .attributes
            .set("linenums".into(), AttributeValue::String(Cow::Borrowed("")));
    }
    if !metadata.options.contains(&"nowrap")
        && state.document_attributes.is_explicit("prewrap")
        && state.document_attributes.get("prewrap").is_none()
    {
        metadata.options.push("nowrap");
    }
}

fn store_named_block_attribute<'input>(
    state: &mut ParserState<'input>,
    metadata: &mut BlockMetadata<'input>,
    name: &'input str,
    value: &'input str,
    quote: AttributeQuote,
    location: Option<Location>,
) {
    match name {
        "title" => {}
        RESERVED_NAMED_ATTRIBUTE_ID => {
            metadata.id = Some(Anchor {
                id: value,
                xreflabel: None,
                location: location.clone().unwrap_or_default(),
                bibliography_label: None,
                bibliography: false,
            });
        }
        RESERVED_NAMED_ATTRIBUTE_ROLE | "roles" => {
            if state.options.compatibility_mode == CompatibilityMode::Asciidoctor {
                metadata.roles.clear();
            }
            metadata.roles.extend(value.split_whitespace());
            if state.options.compatibility_mode == CompatibilityMode::Draft {
                unique(&mut metadata.roles);
            }
        }
        RESERVED_NAMED_ATTRIBUTE_OPTIONS | "options" => {
            if state.options.compatibility_mode == CompatibilityMode::Draft {
                metadata.options.extend(
                    value
                        .split(|c: char| c == ',' || c.is_whitespace())
                        .filter(|value| !value.is_empty()),
                );
                unique(&mut metadata.options);
            } else {
                let value = if value.contains(',') && value.contains(' ') {
                    state.intern_str(&value.replace(' ', ""))
                } else {
                    value
                };
                metadata
                    .options
                    .extend(value.split(',').filter(|value| !value.is_empty()));
            }
        }
        "style" => metadata.style = Some(value),
        RESERVED_NAMED_ATTRIBUTE_SUBS => {
            let spec = parse_subs_attribute(value);
            for entry in spec.unsupported_entries() {
                state.add_warning(Warning::new(
                    WarningKind::ContentRecovery {
                        message: format!("Unsupported block substitution '{entry}' in subs=. The entry is preserved, but its effect is ignored.").into(),
                    },
                    location.clone().map(|location| state.create_error_source_location(location)),
                ));
            }
            metadata.substitutions = Some(spec);
        }
        "attribution" => {
            metadata.attribution = Some(Attribution::new(plain_attribute_value(value, location)));
            metadata.attribution_substitutions = quote == AttributeQuote::Single;
        }
        "citetitle" => {
            metadata.citetitle = Some(CiteTitle::new(plain_attribute_value(value, location)));
            metadata.citetitle_substitutions = quote == AttributeQuote::Single;
        }
        _ => {
            metadata.attributes.set(
                Cow::Borrowed(name),
                AttributeValue::String(Cow::Borrowed(value)),
            );
        }
    }
}

fn record_attribute_position<'a>(
    metadata: &mut BlockMetadata<'a>,
    name: Option<&'a str>,
    position: AttributePosition,
) {
    if let Some(name) = name {
        metadata
            .attribute_positions
            .get_or_insert_with(Default::default)
            .named
            .insert(name, position);
    } else if position.slot == 0 && metadata.first_source.is_some() {
        metadata
            .attribute_positions
            .get_or_insert_with(Default::default)
            .first = Some(position);
    }
}

pub(super) fn parse_block_attribute_list<'input>(
    state: &mut ParserState<'input>,
    source: &'input str,
    content_start: usize,
    mode: BlockAttributeMode,
    offset: usize,
) -> Result<(bool, BlockMetadata<'input>, Option<TitleSource<'input>>), Error> {
    let expanded = expand_attribute_list(
        source,
        &state.document_attributes,
        state.options.compatibility_mode,
    );
    let substituted = &expanded.text;
    let positions_exact = matches!(substituted, Cow::Borrowed(_));
    let scan_list = |text| scan_block_attribute_list(text, state.options.compatibility_mode, mode);
    let authored = (!positions_exact).then(|| scan_list(source));
    let mut metadata = BlockMetadata::default();
    let mut title_source = None;
    let scanned = scan_list(substituted);
    for (slot, attribute) in
        block_attribute_entries(&scanned, state.options.compatibility_mode, mode)
    {
        if attribute.is_ignored(state.options.compatibility_mode) {
            continue;
        }
        let value = state.intern_str(&attribute.value);
        let name = attribute
            .canonical_name()
            .map(|name| state.intern_str(name));
        let position = AttributePosition {
            list_start: content_start + offset,
            slot,
        };
        let span = attribute.source_range(&expanded.source_map, 0..attribute.value.len())?;
        let location = Some(state.create_block_location(
            content_start + span.start,
            content_start + span.end,
            offset,
        ));

        if matches!(name, Some("alt" | "caption")) {
            record_attribute_position(&mut metadata, name, position);
            let original = authored.as_ref().and_then(|attributes| {
                attributes.iter().find(|a| {
                    a.name.as_deref() == name
                        && a.value_start <= span.start
                        && span.end <= a.value_end
                })
            });
            let pending =
                capture_block_content(state, source, content_start, attribute, original, mode);
            if name == Some("alt") {
                metadata.attributes.set("alt".into(), value.into());
                metadata.alt_source = Some(Box::new(pending));
            } else {
                metadata.caption_source = Some(Box::new(pending));
            }
            continue;
        }
        if let Some(name) = name {
            store_named_block_attribute(
                state,
                &mut metadata,
                name,
                value,
                attribute.quote,
                location,
            );
            if name == "title" {
                title_source = Some(TitleSource::Attribute(attribute.resolve_content(
                    value,
                    &expanded.source_map,
                    content_start + offset,
                    Rc::clone(&state.document_attributes),
                )?));
            }
        } else if slot == 0 {
            store_first_block_slot(
                state,
                &mut metadata,
                attribute,
                mode,
                content_start,
                offset,
                &expanded.source_map,
            )?;
        } else {
            ensure_positional_slot(&mut metadata, slot);
            if let Some(positional) = metadata.positional_attributes.get_mut(slot - 1) {
                *positional = PositionalAttribute {
                    position: Some(position),
                    value,
                    quoted_empty: value.is_empty() && attribute.quote != AttributeQuote::Unquoted,
                    substitutions: attribute.quote == AttributeQuote::Single,
                    location,
                };
            }
        }
        record_attribute_position(&mut metadata, name, position);
    }

    if state.options.compatibility_mode == CompatibilityMode::Draft {
        unique(&mut metadata.roles);
        unique(&mut metadata.options);
    }
    let discrete = matches!(metadata.style, Some("discrete" | "float"));
    Ok((discrete, metadata, title_source))
}

fn capture_block_content<'a>(
    state: &ParserState<'a>,
    source: &'a str,
    content_start: usize,
    attribute: &ScannedAttribute,
    original: Option<&ScannedAttribute>,
    mode: BlockAttributeMode,
) -> crate::model::content_attribute::PendingContentAttribute<'a> {
    let value = state.intern_str(&attribute.value);
    let (from, to) = original.map_or((attribute.value_start, attribute.value_end), |a| {
        (a.value_start, a.value_end)
    });
    let formatted =
        matches!(mode, BlockAttributeMode::Block) && attribute.quote == AttributeQuote::Single;
    crate::model::content_attribute::PendingContentAttribute {
        source: if formatted {
            source.get(from..to).unwrap_or(value)
        } else {
            original.map_or(value, |original| state.intern_str(&original.value))
        },
        start: content_start + from,
        end: content_start + to,
        formatted,
        attributes: source
            .contains('{')
            .then(|| Box::new((*state.document_attributes).clone())),
    }
}

fn store_first_block_slot<'a>(
    state: &ParserState<'a>,
    metadata: &mut BlockMetadata<'a>,
    attribute: &ScannedAttribute,
    mode: BlockAttributeMode,
    content_start: usize,
    offset: usize,
    source_map: &SourceMap,
) -> Result<(), Error> {
    let value = state.intern_str(&attribute.value);
    match mode {
        BlockAttributeMode::Block => {
            apply_block_style(
                state,
                metadata,
                attribute,
                source_map,
                content_start,
                offset,
            )?;
            if let Some(part) =
                block_shorthand(value, attribute.quote, state.options.compatibility_mode)
                    .into_iter()
                    .find(|part| matches!(part.kind, ShorthandKind::Style))
            {
                let span = attribute.source_range(source_map, part.range.clone())?;
                metadata.first_source = Some(Box::new(
                    crate::model::content_attribute::PendingContentAttribute {
                        source: &value[part.range],
                        start: content_start + span.start,
                        end: content_start + span.end,
                        formatted: attribute.quote == AttributeQuote::Single,
                        attributes: None,
                    },
                ));
            }
            Ok(())
        }
        BlockAttributeMode::Macro
            if !value.is_empty() || attribute.quote != AttributeQuote::Unquoted =>
        {
            metadata.style = Some(value);
            let span = attribute.source_range(source_map, 0..value.len())?;
            metadata.first_source = Some(Box::new(
                crate::model::content_attribute::PendingContentAttribute {
                    source: value,
                    start: content_start + span.start,
                    end: content_start + span.end,
                    formatted: false,
                    attributes: value
                        .contains('{')
                        .then(|| Box::new((*state.document_attributes).clone())),
                },
            ));
            Ok(())
        }
        BlockAttributeMode::Macro => Ok(()),
    }
}

fn plain_attribute_value(value: &str, location: Option<Location>) -> Vec<InlineNode<'_>> {
    vec![InlineNode::PlainText(Plain {
        content: value,
        location: location.unwrap_or_default(),
        escaped: false,
    })]
}

fn extract_quote_attributes(metadata: &mut BlockMetadata<'_>, mode: CompatibilityMode) {
    let named_attribution = metadata.attribution.take();
    let named_citetitle = metadata.citetitle.take();
    let positional_attribution = metadata.take_next_positional_attribute();
    let positional_citetitle = metadata.take_next_positional_attribute();

    if let Some(attribute) = positional_attribution.filter(|value| {
        (!value.value.is_empty() || (mode == CompatibilityMode::Asciidoctor && value.quoted_empty))
            && positional_wins(metadata, value.position, "attribution", mode, false)
    }) {
        metadata.attribution = Some(Attribution::new(plain_attribute_value(
            attribute.value,
            attribute.location,
        )));
        metadata.attribution_substitutions = attribute.substitutions;
    } else {
        metadata.attribution = named_attribution;
    }

    if let Some(attribute) = positional_citetitle.filter(|value| {
        (!value.value.is_empty() || (mode == CompatibilityMode::Asciidoctor && value.quoted_empty))
            && positional_wins(metadata, value.position, "citetitle", mode, false)
    }) {
        metadata.citetitle = Some(CiteTitle::new(plain_attribute_value(
            attribute.value,
            attribute.location,
        )));
        metadata.citetitle_substitutions = attribute.substitutions;
    } else {
        metadata.citetitle = named_citetitle;
    }
}

fn apply_quote_attribute_substitutions<'input>(
    state: &mut ParserState<'input>,
    metadata: &mut BlockMetadata<'input>,
    offset: usize,
    substitutions: SubstitutionPlan,
) -> Result<(), Error> {
    let block_metadata = BlockParsingMetadata {
        substitutions,
        ..BlockParsingMetadata::default()
    };

    if metadata.attribution_substitutions
        && let Some(InlineNode::PlainText(plain)) = metadata
            .attribution
            .as_deref()
            .and_then(|value| value.first())
    {
        let start = plain.location.absolute_start.saturating_sub(offset);
        let end = plain.location.absolute_end.saturating_sub(offset);
        let (inlines, _) =
            process_inlines(state, &block_metadata, start, end, offset, plain.content)?;
        metadata.attribution = Some(Attribution::new(inlines));
    }
    if metadata.citetitle_substitutions
        && let Some(InlineNode::PlainText(plain)) = metadata
            .citetitle
            .as_deref()
            .and_then(|value| value.first())
    {
        let start = plain.location.absolute_start.saturating_sub(offset);
        let end = plain.location.absolute_end.saturating_sub(offset);
        let (inlines, _) =
            process_inlines(state, &block_metadata, start, end, offset, plain.content)?;
        metadata.citetitle = Some(CiteTitle::new(inlines));
    }
    for (value, formatted) in [
        (
            &mut metadata.attribution,
            metadata.attribution_substitutions,
        ),
        (&mut metadata.citetitle, metadata.citetitle_substitutions),
    ] {
        if !formatted && let Some(value) = value {
            for node in value.inlines_mut() {
                if let InlineNode::PlainText(plain) = node {
                    *node = InlineNode::RawText(crate::Raw {
                        content: plain.content,
                        location: plain.location.clone(),
                        subs: vec![Substitution::SpecialChars],
                    });
                }
            }
        }
    }
    Ok(())
}

pub(super) fn resolve_content_attribute<'a>(
    state: &mut ParserState<'a>,
    source: crate::model::content_attribute::PendingContentAttribute<'a>,
    offset: usize,
) -> Result<ContentAttribute<'a>, Error> {
    let current = source
        .attributes
        .map(|attributes| std::mem::replace(&mut state.document_attributes, Rc::new(*attributes)));
    let result = if source.formatted {
        process_inlines(
            state,
            &BlockParsingMetadata::default(),
            source.start,
            source.end,
            offset,
            source.source,
        )
        .map(|(nodes, _)| ContentAttribute::new(nodes))
    } else {
        let text = substitute(
            source.source,
            &[Substitution::Attributes],
            &state.document_attributes,
        );
        let text = state.intern_str(&text.replace("\\]", "]"));
        Ok(ContentAttribute::new(if text.is_empty() {
            Vec::new()
        } else {
            vec![InlineNode::PlainText(Plain {
                content: text,
                location: state.create_block_location(source.start, source.end, offset),
                escaped: false,
            })]
        }))
    };
    if let Some(current) = current {
        state.document_attributes = current;
    }
    result
}

fn resolve_caption_content<'a>(
    state: &mut ParserState<'a>,
    metadata: &mut BlockMetadata<'a>,
    offset: usize,
) -> Result<(), Error> {
    if let Some(source) = metadata.caption_source.take() {
        let formatted = source.formatted;
        let mut content = resolve_content_attribute(state, *source, offset)?;
        if !formatted {
            for node in content.inlines_mut() {
                if let InlineNode::PlainText(plain) = node {
                    *node = InlineNode::RawText(crate::Raw {
                        content: plain.content,
                        location: plain.location.clone(),
                        subs: vec![Substitution::SpecialChars],
                    });
                }
            }
        }
        metadata.caption = Some(Caption::CustomContent(content));
    }
    Ok(())
}

pub(super) fn resolve_block_content_attributes<'a>(
    state: &mut ParserState<'a>,
    metadata: &mut BlockMetadata<'a>,
    media: Option<&BlockMetadata<'a>>,
    image: bool,
    offset: usize,
) -> Result<Option<ContentAttribute<'a>>, Error> {
    if let Some(media) = media
        && media.caption_source.is_some()
    {
        metadata.caption_source.clone_from(&media.caption_source);
    }
    let alt = if image {
        let mut selected = metadata.clone();
        if let Some(media) = media {
            merge_macro_metadata(&mut selected, media, state.options.compatibility_mode);
        }
        let position = selected
            .attribute_positions
            .as_ref()
            .and_then(|positions| positions.first);
        let macro_first = media
            .and_then(|media| media.first_source.as_ref())
            .is_some();
        let use_first = selected.first_source.is_some()
            && (selected.alt_source.is_none()
                || ((state.options.compatibility_mode == CompatibilityMode::Draft || macro_first)
                    && positional_wins(
                        &selected,
                        position,
                        "alt",
                        state.options.compatibility_mode,
                        true,
                    )));
        if use_first {
            selected.first_source
        } else {
            selected.alt_source
        }
    } else {
        None
    };
    let caption_first = match (&metadata.caption_source, &alt) {
        (Some(caption), Some(alt)) => caption.start < alt.start,
        _ => true,
    };
    if caption_first {
        resolve_caption_content(state, metadata, offset)?;
    }
    let alt = alt
        .map(|source| resolve_content_attribute(state, *source, offset))
        .transpose()?;
    if !caption_first {
        resolve_caption_content(state, metadata, offset)?;
    }
    metadata.alt_source = None;
    if image || media.is_none() {
        metadata.first_source = None;
    }
    Ok(alt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merging_substitutions_distinguishes_absent_and_explicit_empty() {
        for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
            let mut metadata = BlockMetadata {
                substitutions: Some(crate::SubstitutionSpec::Explicit(vec![
                    Substitution::Quotes,
                ])),
                ..BlockMetadata::default()
            };
            let original = metadata.clone();
            merge_attribute_metadata(&mut metadata, BlockMetadata::default(), mode);
            assert_eq!(metadata, original);
            let empty = BlockMetadata {
                substitutions: Some(crate::SubstitutionSpec::Explicit(Vec::new())),
                ..BlockMetadata::default()
            };
            merge_attribute_metadata(&mut metadata, empty.clone(), mode);
            assert_eq!(metadata, empty);
        }
    }
    #[test]
    fn selected_entries_keep_their_order_across_expansion_and_stacked_lists()
    -> Result<(), Box<dyn std::error::Error>> {
        let source = ":entries: width=10,old,width=20\n\n[custom,{entries}]\nFirst.\n\n[custom,{entries}]\n[custom,,last]\nSecond.";
        for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
            let parsed = crate::parse(
                source,
                &crate::Options::builder()
                    .with_compatibility_mode(mode)
                    .build()?,
            )?;
            let first = parsed
                .document()
                .blocks
                .first()
                .and_then(Block::metadata)
                .ok_or("missing first metadata")?;
            let positions = first
                .attribute_positions
                .as_ref()
                .ok_or("missing positions")?;
            let width = positions
                .named
                .get("width")
                .ok_or("missing named position")?;
            let positional = first
                .raw_positional_attributes()
                .get(1)
                .ok_or("missing second slot")?;
            let positional_order = positional.position.ok_or("missing positional order")?;
            let first_order = positions.first.ok_or("missing first slot order")?;
            assert_eq!(first_order.slot, 0);
            assert!(first_order < positional_order);
            assert_eq!(width.slot, 3);
            assert_eq!(positional_order.slot, 2);
            assert!(positional_order < *width);
            assert_eq!(positional.value, "old");
            let location = positional.location.as_ref().ok_or("missing location")?;
            assert_eq!(
                source.get(location.absolute_start..=location.absolute_end),
                Some("{entries}")
            );
            let second = parsed
                .document()
                .blocks
                .last()
                .and_then(Block::metadata)
                .ok_or("missing second metadata")?;
            let width = second
                .attribute_positions
                .as_ref()
                .and_then(|positions| positions.named.get("width"))
                .ok_or("missing named order")?;
            let last = second
                .raw_positional_attributes()
                .get(1)
                .ok_or("missing last slot")?;
            assert_eq!(last.value, "last");
            assert!(last.position.ok_or("missing last order")? > *width);
        }
        Ok(())
    }
}
