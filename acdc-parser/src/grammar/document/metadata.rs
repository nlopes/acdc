//! Block styles, attributes, anchors, and captions.

use crate::{
    Anchor, AttributeValue, Attribution, Block, BlockMetadata, CiteTitle, Error, InlineNode,
    Location, Plain, Title,
    document_attribute::AttributeDeclaration,
    grammar::{
        ParserState,
        attributes::{AttributeQuote, scan_attribute_list},
        helpers::{
            BlockParsingMetadata, MacroAttributeContext, RESERVED_NAMED_ATTRIBUTE_ID,
            RESERVED_NAMED_ATTRIBUTE_OPTIONS, RESERVED_NAMED_ATTRIBUTE_ROLE,
            RESERVED_NAMED_ATTRIBUTE_SUBS, parse_comma_separated_values,
        },
        inline_processing::process_inlines,
    },
    model::{
        Caption, CaptionKind, PositionalAttribute, SectionLevel, Substitution, substitute,
        substitution::SubstitutionPlan,
    },
};
use std::borrow::Cow;

#[cfg(not(feature = "pre-spec-subs"))]
use crate::{Warning, WarningKind};

#[cfg(feature = "pre-spec-subs")]
use crate::model::substitution::parse_subs_attribute;

/// A block metadata line recognized by the document grammar.
#[derive(Debug)]
pub(super) enum BlockMetadataLine<'input> {
    Anchor(Anchor<'input>),
    Attributes((bool, Box<BlockMetadata<'input>>)),
    Title {
        source: &'input str,
        start: usize,
        end: usize,
    },
    DocumentAttribute(AttributeDeclaration<'input>, Location),
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
    for (name, value) in attribute_metadata.attributes.iter() {
        metadata.attributes.set(name.clone(), value.clone());
    }
    metadata.overlay_positional_attributes(&attribute_metadata.positional_attributes);
    #[cfg(feature = "pre-spec-subs")]
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

pub(super) fn finish_block_parsing_metadata<'input>(
    state: &mut ParserState<'input>,
    mut metadata: BlockMetadata<'input>,
    title: Title<'input>,
    parent_section_level: Option<SectionLevel>,
    discrete: bool,
    offset: usize,
) -> Result<BlockParsingMetadata<'input>, Error> {
    let discrete = discrete || matches!(metadata.style, Some("discrete" | "float"));
    #[cfg(feature = "pre-spec-subs")]
    let substitutions = metadata
        .substitutions
        .as_ref()
        .map_or_else(SubstitutionPlan::default, SubstitutionPlan::for_block_spec);
    #[cfg(not(feature = "pre-spec-subs"))]
    let substitutions = SubstitutionPlan::default();
    let hardbreaks = substitutions.enabled(&Substitution::PostReplacements)
        && (state.hardbreaks || metadata.options.contains(&"hardbreaks"));
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
    })
}

// Boundary lookahead must not parse titles or register their inline content.
pub(super) fn metadata_marks_discrete_heading(metadata: &str, state: &ParserState<'_>) -> bool {
    let mut discrete = false;
    for line in metadata.lines() {
        let Some(attributes) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        else {
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
        let attributes = scan_attribute_list(&attributes, &[]);
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

#[derive(Clone, Copy)]
pub(super) enum BlockAttributeMode {
    Block,
    Macro(MacroAttributeContext),
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
    context: Option<MacroAttributeContext>,
) -> Option<(usize, usize)> {
    if quote == AttributeQuote::Unquoted && value == "None" {
        return None;
    }
    let title_position = if name == "title" {
        location
            .as_ref()
            .map(|location| (location.absolute_start, location.absolute_end))
    } else {
        None
    };
    match name {
        RESERVED_NAMED_ATTRIBUTE_ID if metadata.id.is_none() => {
            metadata.id = Some(Anchor {
                id: value,
                xreflabel: None,
                location: location.clone().unwrap_or_default(),
                bibliography_label: None,
                bibliography: false,
            });
        }
        RESERVED_NAMED_ATTRIBUTE_ROLE | "roles" => {
            metadata
                .roles
                .extend(value.split_whitespace().filter(|role| !role.is_empty()));
        }
        RESERVED_NAMED_ATTRIBUTE_OPTIONS | "options" => {
            metadata
                .options
                .extend(parse_comma_separated_values(state, value));
        }
        "style" => metadata.style = Some(value),
        RESERVED_NAMED_ATTRIBUTE_SUBS => {
            #[cfg(feature = "pre-spec-subs")]
            {
                state.add_generic_warning_at(
                    "The subs= attribute may change when the AsciiDoc specification is finalized. See: https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/issues/16".to_string(),
                    location.clone().unwrap_or_default(),
                );
                metadata.substitutions = Some(parse_subs_attribute(value));
            }
            #[cfg(not(feature = "pre-spec-subs"))]
            state.add_warning(Warning::new(
                WarningKind::ContentRecovery {
                    message: "The subs= attribute is not honoured in this build (the `pre-spec-subs` feature is disabled). Requested content substitutions were ignored.".into(),
                },
                Some(state.create_error_source_location(location.clone().unwrap_or_default())),
            ));
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
            if context == Some(MacroAttributeContext::Image) && name == "link" {
                metadata.attributes.set(
                    Cow::Borrowed(name),
                    AttributeValue::String(Cow::Borrowed(value)),
                );
            } else {
                metadata.attributes.insert(
                    Cow::Borrowed(name),
                    AttributeValue::String(Cow::Borrowed(value)),
                );
            }
        }
    }
    title_position
}

pub(super) fn parse_block_attribute_list<'input>(
    state: &mut ParserState<'input>,
    source: &'input str,
    content_start: usize,
    fallback_end: usize,
    mode: BlockAttributeMode,
) -> (bool, BlockMetadata<'input>, Option<(usize, usize)>) {
    let substituted = substitute(
        source,
        &[Substitution::Attributes],
        &state.document_attributes,
    );
    let positions_exact = matches!(substituted, Cow::Borrowed(_));
    let attributes = scan_attribute_list(&substituted, &[]);
    let mut metadata = BlockMetadata::default();
    let mut discrete = false;
    let mut title_position = None;

    for (slot, attribute) in attributes.into_iter().enumerate() {
        let value = state.intern_str(&attribute.value);
        let name = attribute.name.as_deref().map(|name| state.intern_str(name));
        let location = positions_exact.then(|| {
            state.create_location(
                content_start + attribute.value_start,
                content_start + attribute.value_end,
            )
        });

        if let Some(name) = name {
            title_position = store_named_block_attribute(
                state,
                &mut metadata,
                name,
                value,
                attribute.quote,
                location,
                match mode {
                    BlockAttributeMode::Block => None,
                    BlockAttributeMode::Macro(context) => Some(context),
                },
            )
            .or(title_position);
        } else if slot == 0 {
            match mode {
                BlockAttributeMode::Block => {
                    discrete = apply_block_style(state, &mut metadata, value, location.as_ref());
                }
                BlockAttributeMode::Macro(_) if !value.is_empty() => metadata.style = Some(value),
                BlockAttributeMode::Macro(_) => {}
            }
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

    if title_position.is_none() && metadata.attributes.get("title").is_some() {
        title_position = Some((content_start, fallback_end));
    }
    (discrete, metadata, title_position)
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
    Ok(())
}
