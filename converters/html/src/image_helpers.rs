//! Shared helper functions for image rendering (block and inline).

use std::io::Write;

use acdc_parser::BlockMetadata;

use crate::{Error, constants::escape_ampersands, inlines::escape_attribute};

/// Write width and height attributes if present in metadata.
///
/// Checks the metadata attributes for `width` and `height` keys and writes
/// them as HTML attributes if found.
pub(crate) fn write_dimension_attributes<W: Write + ?Sized>(
    w: &mut W,
    metadata: &BlockMetadata,
) -> Result<(), Error> {
    if let Some(width) = metadata.attributes.get("width") {
        write!(w, " width=\"{}\"", escape_attribute(&width.to_string()))?;
    }
    if let Some(height) = metadata.attributes.get("height") {
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
