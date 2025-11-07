//! Shared helper functions for image rendering (block and inline).

use std::{io::Write, path::PathBuf};

use acdc_parser::{BlockMetadata, Source};

use crate::Error;

/// Generate alt text from filename by removing extension and replacing separators.
///
/// This is used as a fallback when no explicit alt text is provided.
/// Converts hyphens and underscores to spaces for better readability.
///
/// # Examples
///
/// - `sunset.jpg` → `"sunset"`
/// - `my-image_file.png` → `"my image file"`
#[must_use]
pub(crate) fn alt_text_from_filename(source: &Source) -> String {
    let mut filepath = PathBuf::from(source.get_filename().unwrap_or(""));
    filepath.set_extension("");
    filepath.to_str().unwrap_or("").replace(['-', '_'], " ")
}

/// Write width and height attributes if present in metadata.
///
/// Checks the metadata attributes for `width` and `height` keys and writes
/// them as HTML attributes if found.
pub(crate) fn write_dimension_attributes<W: Write + ?Sized>(
    w: &mut W,
    metadata: &BlockMetadata,
) -> Result<(), Error> {
    if let Some(width) = metadata.attributes.get("width") {
        write!(w, " width=\"{width}\"")?;
    }
    if let Some(height) = metadata.attributes.get("height") {
        write!(w, " height=\"{height}\"")?;
    }
    Ok(())
}
