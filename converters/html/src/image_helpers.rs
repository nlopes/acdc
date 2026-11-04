//! Shared helper functions for image rendering (block and inline).

use std::io::Write;

use acdc_parser::ast::{AttributeValueRef, AttributesRef};

use crate::{Error, constants::escape_ampersands, inlines::escape_attribute};

/// Write width and height attributes when present.
pub(crate) fn write_dimension_attributes<W: Write + ?Sized>(
    w: &mut W,
    attributes: AttributesRef<'_>,
) -> Result<(), Error> {
    if let Some(width) = attributes.get("width").and_then(attribute_scalar) {
        write!(w, " width=\"{}\"", escape_attribute(width))?;
    }
    if let Some(height) = attributes.get("height").and_then(attribute_scalar) {
        write!(w, " height=\"{}\"", escape_attribute(height))?;
    }
    Ok(())
}

/// Escape authored image text without changing existing character references.
pub(crate) fn escape_image_text(text: &str) -> String {
    escape_ampersands(text)
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub(crate) fn attribute_text<'doc>(
    attributes: AttributesRef<'doc>,
    name: &str,
) -> Option<&'doc str> {
    if let Some(AttributeValueRef::Text(text)) = attributes.get(name) {
        Some(text)
    } else {
        None
    }
}

pub(crate) fn attribute_scalar(value: AttributeValueRef<'_>) -> Option<&str> {
    match value {
        AttributeValueRef::Text(text) => Some(text),
        AttributeValueRef::Bool(value) => Some(if value { "true" } else { "false" }),
        AttributeValueRef::None => Some("null"),
        AttributeValueRef::Content(_)
        | AttributeValueRef::ContentList(_)
        | AttributeValueRef::Tokens(_)
        | AttributeValueRef::Substitutions(_)
        | _ => None,
    }
}
