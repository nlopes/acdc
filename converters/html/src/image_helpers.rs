//! Shared helper functions for image rendering (block and inline).

use std::io::Write;

use acdc_parser::ElementAttributes;

use crate::{Error, constants::escape_ampersands, inlines::escape_attribute};

/// Write width and height attributes when present.
pub(crate) fn write_dimension_attributes<W: Write + ?Sized>(
    w: &mut W,
    attributes: &ElementAttributes,
) -> Result<(), Error> {
    if let Some(width) = attributes.get("width") {
        write!(w, " width=\"{}\"", escape_attribute(&width.to_string()))?;
    }
    if let Some(height) = attributes.get("height") {
        write!(w, " height=\"{}\"", escape_attribute(&height.to_string()))?;
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
