//! Post-processing of generated images.
//!
//! Every image is measured before it is cached: the HTML backend wants
//! `width`/`height` attributes on the `<img>` tag, and re-measuring on each
//! run would mean re-reading every cached file. The dimensions are therefore
//! stored next to the cached image, and only the generation path pays for
//! decoding.
//!
//! SVG additionally gets normalised (see [`svg`]), which is why
//! post-processing returns the image bytes rather than only the size.

mod gif;
mod png;
mod svg;

use crate::{Format, error::Result};

/// An image plus whatever could be measured about it.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct ProcessedImage {
    /// The image bytes to write to disk.
    pub(crate) data: Vec<u8>,
    /// Intrinsic width in pixels, when the format carries one.
    pub(crate) width: Option<f64>,
    /// Intrinsic height in pixels, when the format carries one.
    pub(crate) height: Option<f64>,
}

/// Normalise and measure `data` according to `format`.
///
/// `optimise` strips comments from SVG output; it is off when the document
/// sets the `nooptimise` option, which keeps generator comments for debugging.
///
/// # Errors
///
/// Returns [`Error::Image`](crate::Error::Image) when the bytes do not match
/// the format they claim to be.
pub(crate) fn post_process(
    format: Format,
    data: Vec<u8>,
    optimise: bool,
) -> Result<ProcessedImage> {
    match format {
        Format::Png => {
            let (width, height) = png::dimensions(&data)?;
            Ok(ProcessedImage {
                data,
                width: Some(f64::from(width)),
                height: Some(f64::from(height)),
            })
        }
        Format::Gif => {
            let (width, height) = gif::dimensions(&data)?;
            Ok(ProcessedImage {
                data,
                width: Some(f64::from(width)),
                height: Some(f64::from(height)),
            })
        }
        Format::Svg => svg::post_process(data, optimise),
        // PDF and JPEG carry their size in structures we would have to parse
        // in full (asciidoctor-diagram leaves both unmeasured too), and the
        // text formats have no intrinsic size at all.
        Format::Pdf | Format::Jpeg | Format::Txt | Format::Atxt | Format::Utxt => {
            Ok(ProcessedImage {
                data,
                width: None,
                height: None,
            })
        }
    }
}
