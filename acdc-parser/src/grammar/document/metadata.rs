//! Block styles, attributes, anchors, and captions.

use crate::{
    Anchor, AttributeValue, Attribution, Block, BlockMetadata, CiteTitle, CompatibilityMode,
    ContentAttribute, DocumentAttributes, Error, InlineNode, Location, Plain, Title, Warning,
    WarningKind,
    document_attribute::AttributeDeclaration,
    grammar::{
        ParserState,
        attributes::{
            AttributeQuote, BlockAttributeMode, ScannedAttribute, block_attribute_line,
            block_attribute_list_allowed, scan_block_attribute_list,
        },
        helpers::{
            BlockParsingMetadata, RESERVED_NAMED_ATTRIBUTE_ID, RESERVED_NAMED_ATTRIBUTE_OPTIONS,
            RESERVED_NAMED_ATTRIBUTE_ROLE, RESERVED_NAMED_ATTRIBUTE_SUBS,
            parse_comma_separated_values,
        },
        inline_processing::process_inlines,
    },
    model::{
        Caption, CaptionKind, PositionalAttribute, SectionLevel, Substitution, substitute,
        substitution::{SubstitutionPlan, parse_subs_attribute},
    },
};
use std::{borrow::Cow, rc::Rc};

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
    pub(super) end: usize,
}

#[derive(Clone, Debug)]
pub(super) struct ContentAttributeSource<'a> {
    pub(super) source: &'a str,
    pub(super) value: Option<&'a str>,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) quote: AttributeQuote,
    pub(super) attributes: Option<Rc<DocumentAttributes<'a>>>,
}

pub(super) fn resolve_title<'a>(
    state: &mut ParserState<'a>,
    source: ContentAttributeSource<'a>,
    offset: usize,
) -> Result<Title<'a>, Error> {
    let inlines = if state.options.compatibility_mode == CompatibilityMode::Draft
        && source.quote != AttributeQuote::Single
    {
        let value = source.value.unwrap_or(source.source);
        if value.is_empty() {
            Vec::new()
        } else {
            vec![InlineNode::RawText(crate::Raw {
                content: value,
                location: state.create_block_location(source.start, source.end, offset),
                subs: vec![Substitution::SpecialChars],
            })]
        }
    } else {
        // Named attributes keep the values active when their attribute list was read.
        // Parse the authored source under that snapshot to retain inline locations.
        let current = source
            .attributes
            .map(|attributes| std::mem::replace(&mut state.document_attributes, attributes));
        let result = process_inlines(
            state,
            &BlockParsingMetadata::default(),
            source.start,
            source.end,
            offset,
            source.source,
        );
        if let Some(current) = current {
            state.document_attributes = current;
        }
        result?.0
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
    metadata.append_document_attributes(attribute_metadata.take_document_attributes());
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
    if attribute_metadata.alt_source.is_some()
        && (attribute_metadata.attributes.contains_key("alt")
            || !metadata.attributes.contains_key("alt"))
    {
        metadata.alt_source = attribute_metadata.alt_source;
    }
    if attribute_metadata.caption_source.is_some() {
        metadata.caption_source = attribute_metadata.caption_source;
    }
    metadata.overlay_positional_attributes(&attribute_metadata.positional_attributes);
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
    if mode == CompatibilityMode::Draft {
        merge_attribute_metadata(metadata, attributes.clone(), mode);
    } else {
        metadata.merge(attributes);
    }
}

fn attribute_title_source<'input>(
    state: &ParserState<'input>,
    value: &'input str,
    quote: AttributeQuote,
    start: usize,
    end: usize,
    offset: usize,
) -> ContentAttributeSource<'input> {
    // Keep authored attribute references so inline processing can map them to source.
    let raw = state.input.get(start + offset..end + offset);
    let source = raw.filter(|raw| {
        substitute(raw, &[Substitution::Attributes], &state.document_attributes) == value
    });
    let source = source.unwrap_or(value);
    ContentAttributeSource {
        source,
        value: Some(value),
        start,
        end,
        quote,
        attributes: source
            .contains('{')
            .then(|| Rc::clone(&state.document_attributes)),
    }
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
        end: line_start + end,
    })
}

pub(super) fn finish_block_parsing_metadata<'input>(
    state: &mut ParserState<'input>,
    mut metadata: BlockMetadata<'input>,
    title: Title<'input>,
    parent_section_level: Option<SectionLevel>,
    discrete: bool,
    offset: usize,
) -> Result<BlockParsingMetadata<'input>, Error> {
    let discrete = discrete || matches!(metadata.style, Some("discrete" | "float"));
    let substitutions = metadata
        .substitutions
        .as_ref()
        .map_or_else(SubstitutionPlan::default, SubstitutionPlan::for_block_spec);
    let hardbreaks = substitutions.enabled(&Substitution::PostReplacements)
        && (state.hardbreaks || metadata.options.contains(&"hardbreaks"));
    resolve_caption_content(state, &mut metadata, offset)?;
    extract_source_attributes(state, &mut metadata);
    extract_quote_attributes(&mut metadata);
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
        if attributes.starts_with('[') {
            continue;
        }
        let attributes = substitute(
            attributes,
            &[Substitution::Attributes],
            &state.document_attributes,
        );
        let attributes = scan_block_attribute_list(&attributes, state.options.compatibility_mode);
        if let Some(first) = attributes
            .first()
            .filter(|attribute| attribute.name.is_none())
        {
            let style = first
                .value
                .split(['#', '.', '%'])
                .next()
                .unwrap_or_default();
            if !style.is_empty() {
                discrete = matches!(style, "discrete" | "float");
            }
        }
    }
    discrete
}

fn apply_block_style<'input>(
    state: &ParserState<'input>,
    metadata: &mut BlockMetadata<'input>,
    value: &'input str,
    location: Option<&Location>,
) -> bool {
    if value.is_empty() {
        return false;
    }
    if value.chars().any(char::is_whitespace)
        || !value
            .chars()
            .any(|character| matches!(character, '#' | '.' | '%'))
    {
        metadata.style = Some(value);
        return matches!(value, "discrete" | "float");
    }

    let mut kind = None;
    let mut start = 0;
    for (index, character) in value.char_indices() {
        let next_kind = match character {
            '#' => Some(StylePartKind::Id),
            '.' => Some(StylePartKind::Role),
            '%' => Some(StylePartKind::Option),
            _ => continue,
        };
        apply_style_part(
            state,
            metadata,
            kind,
            &value[start..index],
            start,
            index,
            location,
        );
        kind = next_kind;
        start = index + character.len_utf8();
    }
    apply_style_part(
        state,
        metadata,
        kind,
        &value[start..],
        start,
        value.len(),
        location,
    );
    matches!(metadata.style, Some("discrete" | "float"))
}

#[derive(Clone, Copy)]
enum StylePartKind {
    Id,
    Role,
    Option,
}

fn apply_style_part<'input>(
    state: &ParserState<'input>,
    metadata: &mut BlockMetadata<'input>,
    kind: Option<StylePartKind>,
    value: &'input str,
    start: usize,
    end: usize,
    location: Option<&Location>,
) {
    if value.is_empty() {
        return;
    }
    match kind {
        None => metadata.style = Some(value),
        Some(StylePartKind::Id) => {
            let location = location.map_or_else(Location::default, |location| {
                state.create_location(
                    location.absolute_start + start,
                    location.absolute_start + end,
                )
            });
            metadata.id = Some(Anchor {
                id: value,
                xreflabel: None,
                location,
                bibliography_label: None,
                bibliography: false,
            });
        }
        Some(StylePartKind::Role) => metadata.roles.push(value),
        Some(StylePartKind::Option) => metadata.options.push(value),
    }
}

pub(super) fn extract_media_dimensions(metadata: &mut BlockMetadata<'_>) {
    for (slot, name) in [(0, "width"), (1, "height")] {
        if let Some(value) = metadata
            .positional_attributes
            .get(slot)
            .map(|attribute| attribute.value)
            .filter(|value| !value.is_empty())
        {
            metadata
                .attributes
                .insert(name.into(), AttributeValue::String(Cow::Borrowed(value)));
        }
    }
}

fn ensure_positional_slot(metadata: &mut BlockMetadata<'_>, slot: usize) {
    metadata
        .positional_attributes
        .resize(slot, PositionalAttribute::default());
}

pub(super) fn extract_source_attributes(state: &ParserState<'_>, metadata: &mut BlockMetadata<'_>) {
    if metadata.style != Some("source") {
        return;
    }

    if let Some(language) = metadata
        .positional_attributes
        .first()
        .map(|attribute| attribute.value)
        .filter(|value| !value.is_empty())
    {
        metadata.attributes.set(
            "language".into(),
            AttributeValue::String(Cow::Borrowed(language)),
        );
    }
    if let Some(linenums) = metadata
        .positional_attributes
        .get(1)
        .map(|attribute| attribute.value)
        .filter(|value| !value.is_empty())
    {
        metadata.attributes.set(
            "linenums".into(),
            AttributeValue::String(Cow::Borrowed(linenums)),
        );
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
    if quote == AttributeQuote::Unquoted && value == "None" {
        return;
    }
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
                metadata
                    .options
                    .extend(parse_comma_separated_values(state, value));
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

pub(super) fn parse_block_attribute_list<'input>(
    state: &mut ParserState<'input>,
    source: &'input str,
    content_start: usize,
    fallback_end: usize,
    mode: BlockAttributeMode,
    offset: usize,
) -> (
    bool,
    BlockMetadata<'input>,
    Option<ContentAttributeSource<'input>>,
) {
    let substituted = substitute(
        source,
        &[Substitution::Attributes],
        &state.document_attributes,
    );
    let positions_exact = matches!(substituted, Cow::Borrowed(_));
    let scan = |text| scan_block_attribute_list(text, state.options.compatibility_mode);
    let authored = (!positions_exact).then(|| scan(source));
    let mut metadata = BlockMetadata::default();
    let mut discrete = false;
    let mut title_source = None;

    for (slot, attribute) in scan(&substituted).into_iter().enumerate() {
        let value = state.intern_str(&attribute.value);
        let name = attribute.name.as_deref().map(|name| state.intern_str(name));
        let location = positions_exact.then(|| {
            state.create_location(
                content_start + attribute.value_start,
                content_start + attribute.value_end,
            )
        });

        if matches!(name, Some("alt" | "caption"))
            && !(attribute.quote == AttributeQuote::Unquoted && value == "None")
        {
            let original = authored
                .as_ref()
                .and_then(|attributes| attributes.iter().rev().find(|a| a.name.as_deref() == name));
            let pending =
                capture_block_content(state, source, content_start, &attribute, original, mode);
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
            if name == "title" && !(attribute.quote == AttributeQuote::Unquoted && value == "None")
            {
                let original = positions_exact.then_some(&attribute).or_else(|| {
                    authored.as_ref().and_then(|attributes| {
                        attributes.iter().rev().find(|candidate| {
                            candidate.name.as_deref() == Some("title")
                                && substitute(
                                    &candidate.value,
                                    &[Substitution::Attributes],
                                    &state.document_attributes,
                                ) == value
                        })
                    })
                });
                let (start, end) = original.map_or((content_start, fallback_end), |original| {
                    (
                        content_start + original.value_start,
                        content_start + original.value_end,
                    )
                });
                title_source = Some(attribute_title_source(
                    state,
                    value,
                    attribute.quote,
                    start,
                    end,
                    offset,
                ));
            }
        } else if slot == 0 {
            discrete = store_first_block_slot(
                state,
                &mut metadata,
                value,
                location.as_ref(),
                &attribute,
                mode,
                content_start,
            );
        } else {
            ensure_positional_slot(&mut metadata, slot);
            if let Some(positional) = metadata.positional_attributes.get_mut(slot - 1) {
                *positional = PositionalAttribute {
                    value,
                    substitutions: attribute.quote == AttributeQuote::Single,
                    location,
                };
            }
        }
    }

    if state.options.compatibility_mode == CompatibilityMode::Draft {
        unique(&mut metadata.roles);
        unique(&mut metadata.options);
    }
    (discrete, metadata, title_source)
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
    value: &'a str,
    location: Option<&Location>,
    attribute: &ScannedAttribute,
    mode: BlockAttributeMode,
    content_start: usize,
) -> bool {
    match mode {
        BlockAttributeMode::Block => {
            let discrete = apply_block_style(state, metadata, value, location);
            if let Some(style) = metadata.style {
                metadata.alt_source = Some(Box::new(
                    crate::model::content_attribute::PendingContentAttribute {
                        source: style,
                        start: content_start + attribute.value_start,
                        end: content_start + attribute.value_end,
                        formatted: attribute.quote == AttributeQuote::Single,
                        attributes: None,
                    },
                ));
            }
            discrete
        }
        BlockAttributeMode::Macro
            if !value.is_empty() || attribute.quote != AttributeQuote::Unquoted =>
        {
            metadata.style = Some(value);
            metadata.alt_source = Some(Box::new(
                crate::model::content_attribute::PendingContentAttribute {
                    source: value,
                    start: content_start + attribute.value_start,
                    end: content_start + attribute.value_end,
                    formatted: false,
                    attributes: value
                        .contains('{')
                        .then(|| Box::new((*state.document_attributes).clone())),
                },
            ));
            false
        }
        BlockAttributeMode::Macro => false,
    }
}

fn plain_attribute_value(value: &str, location: Option<Location>) -> Vec<InlineNode<'_>> {
    vec![InlineNode::PlainText(Plain {
        content: value,
        location: location.unwrap_or_default(),
        escaped: false,
    })]
}

fn extract_quote_attributes(metadata: &mut BlockMetadata<'_>) {
    if !matches!(metadata.style, Some("quote" | "verse")) {
        return;
    }

    let named_attribution = metadata.attribution.take();
    let named_citetitle = metadata.citetitle.take();
    let positional_attribution = metadata.positional_attributes.first().cloned();
    let positional_citetitle = metadata.positional_attributes.get(1).cloned();

    if let Some(attribute) = positional_attribution.filter(|value| !value.value.is_empty()) {
        metadata.attribution = Some(Attribution::new(plain_attribute_value(
            attribute.value,
            attribute.location,
        )));
        metadata.attribution_substitutions = attribute.substitutions;
    } else {
        metadata.attribution = named_attribution;
    }

    if let Some(attribute) = positional_citetitle.filter(|value| !value.value.is_empty()) {
        metadata.citetitle = Some(CiteTitle::new(plain_attribute_value(
            attribute.value,
            attribute.location,
        )));
        metadata.citetitle_substitutions = attribute.substitutions;
    } else {
        metadata.citetitle = named_citetitle;
    }

    let _ = metadata.take_positional_attributes::<2>();
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
    let alt = image
        .then(|| {
            media
                .and_then(|media| media.alt_source.as_ref())
                .or(metadata.alt_source.as_ref())
                .cloned()
        })
        .flatten();
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
    Ok(alt)
}
