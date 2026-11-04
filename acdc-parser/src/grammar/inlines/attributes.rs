//! Inline shorthand and macro attributes.

use std::{borrow::Cow, ops::Range};

use crate::{
    Anchor, BlockMetadata, ContentAttribute, ElementId, Error, InlineMetadata,
    grammar::{
        ParserState,
        attributes::{AttributeQuote, ScannedAttribute, scan_attribute_list},
        helpers::{BlockParsingMetadata, parse_comma_separated_values},
        inline_preprocessor::{ProcessedKind, SourceMap},
    },
    model::{MacroAttributes, PositionalAttribute},
};

/// Attribute shorthand syntax for inline formatting attributes.
#[derive(Debug)]
pub(super) enum Shorthand<'input> {
    Id(ElementId<'input>),
    Role(Cow<'input, str>),
}

fn media_metadata<'input>(
    attributes: &[ScannedAttribute],
    state: &ParserState<'input>,
    start: usize,
) -> BlockMetadata<'input> {
    let mut metadata = BlockMetadata::default();
    for (slot, attribute) in attributes.iter().enumerate() {
        if attribute.is_ignored(state.options.compatibility_mode) {
            continue;
        }
        let value = attribute.value.as_str();
        match attribute.canonical_name() {
            Some("id") => {
                metadata.id = Some(Anchor {
                    reftext_source: None,
                    bibliography_label: None,
                    id: state.intern_str(value),
                    xreflabel: None,
                    location: state.create_location(
                        start + attribute.value_start,
                        start + attribute.value_end,
                    ),
                    bibliography: false,
                });
            }
            Some("role") => {
                metadata.roles = value
                    .split_whitespace()
                    .map(|role| state.intern_str(role))
                    .collect();
            }
            Some("opts") => metadata
                .options
                .extend(parse_comma_separated_values(state, value)),
            Some("subs" | "alt" | "title") => {}
            Some(name) => metadata.attributes.set(
                state.intern_str(name).into(),
                state.intern_str(value).into(),
            ),
            None if slot == 0 => {
                if !value.is_empty() || attribute.quote != AttributeQuote::Unquoted {
                    metadata.style = Some(state.intern_str(value));
                }
            }
            None => {
                metadata
                    .positional_attributes
                    .resize(slot, PositionalAttribute::default());
                if let Some(positional) = metadata.positional_attributes.get_mut(slot - 1) {
                    positional.value = state.intern_str(value);
                }
            }
        }
    }
    metadata
}

/// Parse a selected content value without losing its authored escape positions.
pub(super) fn parse_content<'a>(
    state: &mut ParserState<'a>,
    source: &'a str,
    start: usize,
    quote: AttributeQuote,
) -> Result<ContentAttribute<'a>, Error> {
    let metadata = BlockParsingMetadata {
        substitutions: state.inline_ctx.substitutions,
        ..BlockParsingMetadata::default()
    };
    let parsed = if source.contains("\\]") {
        let mut mapping = SourceMap::default();
        for (position, _) in source.match_indices("\\]") {
            mapping.add_replacement(position, position + 2, 1, ProcessedKind::Escape);
        }
        let text = state.intern_str(&source.replace("\\]", "]"));
        super::text::parse_bracket_label(state, &metadata, text, start, quote, Some(&mapping))
    } else {
        super::text::parse_bracket_label(state, &metadata, source, start, quote, None)
    };
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
    let attributes = scan_attribute_list(content.raw, &content.protected);
    let mut metadata = media_metadata(&attributes, state, start);
    let mut alt = None;
    let mut tooltip = None;
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
    if let Some(id) = &mut metadata.id {
        // Attribute scanning uses an exclusive end; locations use an inclusive end.
        id.location = state.create_location(
            id.location.absolute_start,
            id.location
                .absolute_end
                .saturating_sub(1)
                .max(id.location.absolute_start),
        );
    }
    if icon {
        if let Some(size) = metadata.style.take() {
            metadata.attributes.insert("size".into(), size.into());
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

pub(super) fn insert_named_metadata<'a>(
    state: &ParserState<'a>,
    metadata: &mut InlineMetadata<'a>,
    attributes: &mut MacroAttributes<'a>,
    name: Cow<'a, str>,
    value: String,
    range: Range<usize>,
) {
    match name.as_ref() {
        "id" => {
            metadata.id = Some(ElementId {
                id: state.intern_str(&value),
                location: state
                    .create_location(range.start, range.end.saturating_sub(1).max(range.start)),
            });
        }
        "role" => {
            metadata.roles = value
                .split_whitespace()
                .map(|role| state.intern_str(role))
                .collect();
        }
        "options" => {
            attributes.options =
                crate::grammar::helpers::parse_comma_separated_values(state, &value);
        }
        _ => attributes.attributes.set(name, value.into()),
    }
}
