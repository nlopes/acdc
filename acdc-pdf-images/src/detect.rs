use std::io::Cursor;

use image::{ImageFormat, ImageReader, Limits};

use crate::error::Error;

const MAX_DECODED_IMAGE_BYTES: u64 = 256 * 1024 * 1024;

/// The detected format of some image bytes.
pub(super) struct Detected {
    pub(super) extension: &'static str,
}

/// Detect an image's format from its bytes (magic bytes, not any URL
/// extension). SVG is sniffed separately because Typst renders it natively but
/// the `image` crate cannot decode it.
///
/// Both paths validate before returning so a header-valid but broken image is
/// reported as a per-image failure rather than aborting Typst compilation for
/// the entire document: raster images are fully decoded, and SVGs are parsed
/// with `usvg` — the same parser Typst uses, so anything accepted here renders.
pub(super) fn detect(bytes: &[u8]) -> Result<Detected, Error> {
    detect_with_limit(bytes, MAX_DECODED_IMAGE_BYTES)
}

fn detect_with_limit(bytes: &[u8], max_decoded_bytes: u64) -> Result<Detected, Error> {
    if bytes.is_empty() {
        return Err(Error::Empty);
    }
    if looks_like_svg(bytes) {
        let options = usvg::Options {
            image_href_resolver: usvg::ImageHrefResolver {
                resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
                resolve_string: Box::new(|_, _| None),
            },
            ..usvg::Options::default()
        };
        usvg::Tree::from_data(bytes, &options).map_err(|error| Error::Svg(error.to_string()))?;
        return Ok(Detected { extension: "svg" });
    }

    let format = image::guess_format(bytes).map_err(|_| Error::UnknownFormat)?;
    let extension = extension_for(format).ok_or(Error::UnknownFormat)?;
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_alloc = Some(max_decoded_bytes);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|error| Error::Decode(error.to_string()))?;
    Ok(Detected { extension })
}

/// The canonical file extension for a detected format (e.g. `png`, `jpg`).
fn extension_for(format: ImageFormat) -> Option<&'static str> {
    format.extensions_str().first().copied()
}

/// Heuristic SVG sniff: after any BOM/whitespace, the content starts with an
/// XML declaration or an `<svg` tag.
fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = bytes.get(..1024).unwrap_or(bytes);
    let text = String::from_utf8_lossy(head);
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    trimmed.starts_with("<svg") || (trimmed.starts_with("<?xml") && trimmed.contains("<svg"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoded_raster_limit_is_applied() -> Result<(), base64::DecodeError> {
        let png = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABAQMAAAAl21bKAAAAA1BMVEXyVTNpJlJjAAAACklEQVQI12NgAAAAAgAB4iG8MwAAAABJRU5ErkJggg==",
        )?;

        assert!(matches!(detect_with_limit(&png, 0), Err(Error::Decode(_))));
        Ok(())
    }
}
