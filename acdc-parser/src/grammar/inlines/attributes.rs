//! Inline shorthand and macro attributes.

use std::borrow::Cow;

use crate::{
    Anchor, AttributeValue, BlockMetadata, ContentAttribute, Error,
    grammar::{
        ParserState,
        attributes::{AttributeQuote, scan_attribute_list},
        helpers::{
            MacroAttributeContext, RESERVED_NAMED_ATTRIBUTE_ID, RESERVED_NAMED_ATTRIBUTE_OPTIONS,
            RESERVED_NAMED_ATTRIBUTE_ROLE, RESERVED_NAMED_ATTRIBUTE_SUBS,
            parse_comma_separated_values,
        },
    },
    model::PositionalAttribute,
};

/// Attribute shorthand syntax for inline formatting attributes.
#[derive(Debug)]
pub(super) enum Shorthand<'input> {
    Id(Cow<'input, str>),
    Role(Cow<'input, str>),
}

/// Store parsed inline macro attributes in `BlockMetadata`.
///
/// Returns the title position if a `title=` attribute was found.
pub(super) fn process_attribute_list<'input>(
    attrs: impl IntoIterator<
        Item = Option<(
            Cow<'input, str>,
            AttributeValue<'input>,
            Option<(usize, usize)>,
        )>,
    >,
    metadata: &mut BlockMetadata<'input>,
    state: &ParserState<'input>,
    fallback_start: usize,
    fallback_end: usize,
    context: MacroAttributeContext,
) -> Option<(usize, usize)> {
    let mut title_position = None;
    let mut first_positional = true;

    for (key, value, pos) in attrs.into_iter().flatten() {
        match key.as_ref() {
            k if k == RESERVED_NAMED_ATTRIBUTE_ID
                && (metadata.id.is_none() || context == MacroAttributeContext::Image) =>
            {
                let (id_start, id_end) = pos.unwrap_or((fallback_start, fallback_end));
                let id: &'input str = match value {
                    AttributeValue::String(s) => state.intern_cow(s),
                    AttributeValue::Bool(_) | AttributeValue::None => {
                        state.intern_fmt(format_args!("{value}"))
                    }
                };
                metadata.id = Some(Anchor {
                    bibliography_label: None,
                    id,
                    xreflabel: None,
                    location: state.create_location(id_start, id_end),
                    bibliography: false,
                });
            }
            k if k == RESERVED_NAMED_ATTRIBUTE_ROLE => {
                if let AttributeValue::String(ref s) = value {
                    if context == MacroAttributeContext::Image {
                        metadata.roles.clear();
                    }
                    // Roles are space-separated (not comma-separated) per asciidoctor behavior.
                    // `role='a b'` → two roles; `role='a,b'` → one role containing a comma.
                    for role in s.split_whitespace() {
                        if !role.is_empty() {
                            metadata.roles.push(state.intern_str(role));
                        }
                    }
                }
            }
            k if k == RESERVED_NAMED_ATTRIBUTE_OPTIONS => {
                if let AttributeValue::String(ref s) = value {
                    metadata
                        .options
                        .extend(parse_comma_separated_values(state, s));
                }
            }
            // Inline macro attributes do not set the block's substitution plan.
            k if k == RESERVED_NAMED_ATTRIBUTE_SUBS => {}
            "title" => {
                if let AttributeValue::String(ref s) = value {
                    if pos.is_some() {
                        title_position = pos;
                    }
                    metadata
                        .attributes
                        .set(key, AttributeValue::String(s.clone()));
                }
            }
            _ => {
                if let AttributeValue::String(ref s) = value {
                    if context == MacroAttributeContext::Image {
                        metadata
                            .attributes
                            .set(key, AttributeValue::String(s.clone()));
                    } else {
                        metadata
                            .attributes
                            .insert(key, AttributeValue::String(s.clone()));
                    }
                } else if value == AttributeValue::None {
                    // Positional attribute
                    let key_str: &'input str = state.intern_cow(key);
                    if first_positional {
                        metadata.style = Some(key_str);
                        first_positional = false;
                    } else {
                        metadata.positional_attributes.push(PositionalAttribute {
                            position: None,
                            value: key_str,
                            quoted_empty: false,
                            substitutions: false,
                            location: None,
                        });
                    }
                }
            }
        }
    }

    title_position
}

/// Image attributes keep empty positional slots and let named values override them.
pub(super) fn process_image_attribute_list<'input>(
    source: &str,
    protected: &[std::ops::Range<usize>],
    state: &ParserState<'input>,
    start: usize,
    end: usize,
) -> BlockMetadata<'input> {
    let mut metadata = BlockMetadata::default();
    let mut named = Vec::new();
    for (slot, attribute) in scan_attribute_list(source, protected)
        .into_iter()
        .enumerate()
    {
        if attribute.is_ignored(state.options.compatibility_mode) {
            continue;
        }
        let value = state.intern_str(&attribute.value);
        if let Some(name) = attribute.canonical_name() {
            named.push(Some((
                Cow::Borrowed(state.intern_str(name)),
                AttributeValue::String(Cow::Borrowed(value)),
                Some((start + attribute.value_start, start + attribute.value_end)),
            )));
        } else if slot == 0 {
            if !value.is_empty() || attribute.quote != AttributeQuote::Unquoted {
                metadata.style = Some(value);
            }
        } else {
            metadata
                .positional_attributes
                .resize(slot, PositionalAttribute::default());
            if let Some(positional) = metadata.positional_attributes.get_mut(slot - 1) {
                positional.value = value;
            }
        }
    }
    process_attribute_list(
        named,
        &mut metadata,
        state,
        start,
        end,
        MacroAttributeContext::Image,
    );
    metadata
}

/// Parse a selected content value without losing its authored escape positions.
pub(super) fn parse_content<'a>(
    state: &mut ParserState<'a>,
    source: &'a str,
    start: usize,
    quote: AttributeQuote,
) -> Result<ContentAttribute<'a>, Error> {
    use crate::grammar::{
        helpers::BlockParsingMetadata, inline_processing::process_inlines_no_autolinks,
        state::InlineRules,
    };
    let rules = state.inline_ctx.rules;
    state.inline_ctx.rules.insert(InlineRules::BRACKET_LABEL);
    state.inline_ctx.rules.set(
        InlineRules::DOUBLE_QUOTED_LINK,
        quote == AttributeQuote::Double,
    );
    state.inline_ctx.rules.set(
        InlineRules::SINGLE_QUOTED_LINK,
        quote == AttributeQuote::Single,
    );
    let metadata = BlockParsingMetadata {
        substitutions: state.inline_ctx.substitutions,
        ..BlockParsingMetadata::default()
    };
    let parsed = if source.contains("\\]") {
        let mut mapping = crate::grammar::inline_preprocessor::SourceMap::default();
        for (position, _) in source.match_indices("\\]") {
            mapping.add_replacement(
                position,
                position + 2,
                1,
                crate::grammar::inline_preprocessor::ProcessedKind::Escape,
            );
        }
        let text = state.intern_str(&source.replace("\\]", "]"));
        super::text::process_unescaped_label(state, &metadata, text, start, &mapping)
    } else {
        process_inlines_no_autolinks(
            state,
            &metadata,
            start,
            start + source.len(),
            state.inline_ctx.offset,
            source,
        )
    };
    state.inline_ctx.rules = rules;
    parsed.map(ContentAttribute::new)
}

pub(super) struct MediaContent<'a> {
    pub(super) metadata: BlockMetadata<'a>,
    pub(super) alt: Option<ContentAttribute<'a>>,
    pub(super) tooltip: Option<ContentAttribute<'a>>,
}

pub(super) fn process_media_content<'a>(
    state: &mut ParserState<'a>,
    content: &super::links::LinkContent<'a>,
    start: usize,
    icon: bool,
) -> Result<MediaContent<'a>, Error> {
    let mut metadata = process_image_attribute_list(
        content.raw,
        &content.protected,
        state,
        start,
        start + content.raw.len(),
    );
    let mut alt = None;
    let mut tooltip = None;
    let attributes = scan_attribute_list(content.raw, &content.protected);
    for (slot, attribute) in attributes.iter().enumerate() {
        if attribute.is_ignored(state.options.compatibility_mode) {
            continue;
        }
        match attribute.name.as_deref() {
            Some("alt") => alt = Some(attribute),
            Some("title") => tooltip = Some(attribute),
            None if slot == 0
                && !icon
                && (!attribute.value.is_empty() || attribute.quote != AttributeQuote::Unquoted) =>
            {
                alt = alt.or(Some(attribute));
            }
            _ => {}
        }
    }
    let mut parse = |attribute: &crate::grammar::attributes::ScannedAttribute| {
        parse_content(
            state,
            &content.raw[attribute.value_start..attribute.value_end],
            start + attribute.value_start,
            attribute.quote,
        )
    };
    let tooltip_first = matches!((alt, tooltip), (Some(alt), Some(tooltip)) if tooltip.value_start < alt.value_start);
    let (alt, tooltip) = if tooltip_first {
        let tooltip = tooltip.map(&mut parse).transpose()?;
        (alt.map(parse).transpose()?, tooltip)
    } else {
        let alt = alt.map(&mut parse).transpose()?;
        (alt, tooltip.map(parse).transpose()?)
    };
    metadata.attributes.remove("alt");
    metadata.attributes.remove("title");
    if icon {
        if let Some(size) = metadata.style.take() {
            metadata.attributes.insert("size".into(), size.into());
        }
        if !metadata.roles.is_empty() {
            metadata
                .attributes
                .set("role".into(), metadata.roles.join(" ").into());
        }
        if !metadata.options.is_empty() {
            metadata
                .attributes
                .set("options".into(), metadata.options.join(",").into());
        }
    } else {
        metadata.style = None;
        for name in ["width", "height"] {
            if let Some(value) = metadata
                .take_next_positional_attribute()
                .filter(|value| !value.value.is_empty())
            {
                metadata.attributes.insert(name.into(), value.value.into());
            }
        }
    }
    metadata.move_positional_attributes_to_attributes();
    Ok(MediaContent {
        metadata,
        alt,
        tooltip,
    })
}
