//! Inline shorthand and macro attributes.

use std::borrow::Cow;

use crate::{
    Anchor, AttributeValue, BlockMetadata,
    grammar::{
        ParserState,
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
            k if k == RESERVED_NAMED_ATTRIBUTE_ID && metadata.id.is_none() => {
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
                        .insert(key, AttributeValue::String(s.clone()));
                }
            }
            _ => {
                if let AttributeValue::String(ref s) = value {
                    if context == MacroAttributeContext::Image && key == "link" {
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
                            value: key_str,
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
