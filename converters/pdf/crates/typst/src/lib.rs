//! Shared Typst-generation utilities for `acdc-pdf` converters.
//!
//! This crate is source-format-agnostic: it provides the [`Writer`] and escaping
//! helpers a converter uses to build Typst *body* markup, the document-level
//! [`EmitOptions`], and the theme-driven [`preamble`] (page setup, `#set`/`#show`
//! rules, header/footer/watermark). Each converter supplies its own body walk;
//! everything here is reused.
#![forbid(unsafe_code)]

mod escape;
pub mod preamble;
mod writer;

pub use writer::Writer;

/// An invalid Typst-generation option.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The language is not a two- or three-letter ISO 639 code.
    #[error("invalid language code `{value}`: expected two or three ASCII letters")]
    InvalidLanguage {
        /// Rejected language code.
        value: String,
    },
    /// The region is not a two-letter ISO 3166-1 alpha-2 code.
    #[error("invalid region code `{value}`: expected two ASCII letters")]
    InvalidRegion {
        /// Rejected region code.
        value: String,
    },
}

/// An invalid page dimension or margin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PageGeometryError {
    /// A custom page width or height is not a finite positive value.
    #[error("custom page dimensions must be finite positive point values")]
    InvalidDimensions,
    /// A page margin is negative or is not finite.
    #[error("page margins must be finite non-negative point values")]
    InvalidMargins,
}

/// Metadata embedded in the generated document.
#[derive(Debug, Clone, Default)]
pub struct DocumentMetadata {
    /// Full plain-text document title.
    pub title: Option<String>,
    /// Plain-text author names.
    pub authors: Vec<String>,
    /// Plain-text document description. PDF export writes this as the subject.
    pub description: Option<String>,
    /// Document keywords, kept as one source-defined string.
    pub keywords: Option<String>,
}

impl DocumentMetadata {
    fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.authors.is_empty()
            && self.description.is_none()
            && self.keywords.is_none()
    }
}

/// A Typst-compatible document language and optional region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentLocale {
    language: String,
    region: Option<String>,
}

impl DocumentLocale {
    /// Build a locale from an ISO 639 language code and an optional ISO 3166-1
    /// alpha-2 region code.
    ///
    /// Language codes become lowercase and region codes become uppercase.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidLanguage`] or [`Error::InvalidRegion`] when a
    /// code has an unsupported length or contains non-ASCII letters.
    pub fn try_from_codes(language: &str, region: Option<&str>) -> Result<Self, Error> {
        if !matches!(language.len(), 2..=3)
            || !language.bytes().all(|byte| byte.is_ascii_alphabetic())
        {
            return Err(Error::InvalidLanguage {
                value: language.to_owned(),
            });
        }
        if let Some(region) = region
            && (region.len() != 2 || !region.bytes().all(|byte| byte.is_ascii_alphabetic()))
        {
            return Err(Error::InvalidRegion {
                value: region.to_owned(),
            });
        }

        Ok(Self {
            language: language.to_ascii_lowercase(),
            region: region.map(str::to_ascii_uppercase),
        })
    }
}

impl Default for DocumentLocale {
    fn default() -> Self {
        Self {
            language: "en".to_owned(),
            region: None,
        }
    }
}

/// Document-level options that shape the generated markup.
#[derive(Debug, Clone)]
pub struct EmitOptions {
    /// Metadata embedded in the generated document.
    pub metadata: DocumentMetadata,
    /// Language and optional region applied to document text.
    pub locale: DocumentLocale,
    /// Standard or custom size used for every document page.
    pub page: PageSize,
    /// Portrait or landscape layout used for every document page.
    pub page_layout: PageLayout,
    /// Page margins that override the theme when set.
    pub page_margin: Option<PageMargins>,
    /// Strip branding chrome (page background, header, footer).
    pub plain: bool,
    /// Whether brand fonts are available at render time. When set, the brand
    /// family is named first in each font stack; otherwise only the bundled
    /// fallbacks are named (so Typst is never asked for an absent font).
    pub brand_fonts: bool,
    /// Text shown in the branded running header when set.
    pub running_header_title: Option<String>,
    /// Virtual path of the header logo (registered with the renderer), if any.
    pub logo: Option<String>,
    /// Diagonal gray watermark text stamped on every page, if set. Shown
    /// regardless of `plain`.
    pub watermark: Option<String>,
    /// An optional timestamp shown in the footer's right slot.
    pub watermark_timestamp: Option<String>,
}

impl Default for EmitOptions {
    fn default() -> Self {
        EmitOptions {
            metadata: DocumentMetadata::default(),
            locale: DocumentLocale::default(),
            page: PageSize::A4,
            page_layout: PageLayout::Portrait,
            page_margin: None,
            plain: false,
            brand_fonts: false,
            running_header_title: None,
            logo: None,
            watermark: None,
            watermark_timestamp: None,
        }
    }
}

/// A standard or custom page size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageSize {
    /// ISO A3 paper.
    A3,
    /// ISO A4 paper.
    A4,
    /// ISO A5 paper.
    A5,
    /// US Executive paper.
    Executive,
    /// US Legal paper.
    Legal,
    /// US Letter paper.
    Letter,
    /// US Tabloid paper.
    Tabloid,
    /// Validated custom portrait dimensions.
    Custom(PageDimensions),
}

/// A supported document page layout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PageLayout {
    #[default]
    Portrait,
    Landscape,
}

impl PageSize {
    /// Creates a custom page size from portrait width and height in PDF points.
    ///
    /// # Errors
    ///
    /// Returns an error when either dimension is not finite or is not positive.
    pub const fn try_custom(
        width_points: f64,
        height_points: f64,
    ) -> Result<Self, PageGeometryError> {
        match PageDimensions::try_new(width_points, height_points) {
            Ok(dimensions) => Ok(Self::Custom(dimensions)),
            Err(error) => Err(error),
        }
    }

    /// Returns the page dimensions after applying `layout`.
    #[must_use]
    pub const fn dimensions(self, layout: PageLayout) -> PageDimensions {
        let dimensions = match self {
            Self::A3 => PageDimensions::known(841.89, 1190.551),
            Self::A4 => PageDimensions::known(595.276, 841.89),
            Self::A5 => PageDimensions::known(419.528, 595.276),
            Self::Executive => PageDimensions::known(522.0, 756.0),
            Self::Legal => PageDimensions::known(612.0, 1008.0),
            Self::Letter => PageDimensions::known(612.0, 792.0),
            Self::Tabloid => PageDimensions::known(792.0, 1224.0),
            Self::Custom(dimensions) => dimensions,
        };
        match layout {
            PageLayout::Portrait => dimensions,
            PageLayout::Landscape => dimensions.flipped(),
        }
    }

    /// Returns the page width after applying `layout`, in PDF points.
    #[must_use]
    pub const fn width_points(self, layout: PageLayout) -> f64 {
        self.dimensions(layout).width_points()
    }

    /// Returns the page height after applying `layout`, in PDF points.
    #[must_use]
    pub const fn height_points(self, layout: PageLayout) -> f64 {
        self.dimensions(layout).height_points()
    }
}

/// Validated custom page dimensions in PDF points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageDimensions {
    width: f64,
    height: f64,
}

impl PageDimensions {
    const fn known(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    /// Creates custom portrait dimensions.
    ///
    /// # Errors
    ///
    /// Returns an error when either dimension is not finite or is not positive.
    pub const fn try_new(width_points: f64, height_points: f64) -> Result<Self, PageGeometryError> {
        if !width_points.is_finite()
            || width_points <= 0.0
            || !height_points.is_finite()
            || height_points <= 0.0
        {
            return Err(PageGeometryError::InvalidDimensions);
        }
        Ok(Self::known(width_points, height_points))
    }

    /// Returns the width in PDF points.
    #[must_use]
    pub const fn width_points(self) -> f64 {
        self.width
    }

    /// Returns the height in PDF points.
    #[must_use]
    pub const fn height_points(self) -> f64 {
        self.height
    }

    const fn flipped(self) -> Self {
        Self::known(self.height, self.width)
    }
}

/// Validated page margins in PDF points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageMargins {
    top: f64,
    right: f64,
    bottom: f64,
    left: f64,
}

impl PageMargins {
    /// Creates page margins in top, right, bottom, and left order.
    ///
    /// # Errors
    ///
    /// Returns an error when any margin is negative or is not finite.
    pub const fn try_new(
        top_points: f64,
        right_points: f64,
        bottom_points: f64,
        left_points: f64,
    ) -> Result<Self, PageGeometryError> {
        if !top_points.is_finite()
            || top_points < 0.0
            || !right_points.is_finite()
            || right_points < 0.0
            || !bottom_points.is_finite()
            || bottom_points < 0.0
            || !left_points.is_finite()
            || left_points < 0.0
        {
            return Err(PageGeometryError::InvalidMargins);
        }
        Ok(Self {
            top: top_points,
            right: right_points,
            bottom: bottom_points,
            left: left_points,
        })
    }

    /// Returns the top margin in PDF points.
    #[must_use]
    pub const fn top_points(self) -> f64 {
        self.top
    }

    /// Returns the right margin in PDF points.
    #[must_use]
    pub const fn right_points(self) -> f64 {
        self.right
    }

    /// Returns the bottom margin in PDF points.
    #[must_use]
    pub const fn bottom_points(self) -> f64 {
        self.bottom
    }

    /// Returns the left margin in PDF points.
    #[must_use]
    pub const fn left_points(self) -> f64 {
        self.left
    }
}

#[cfg(test)]
mod tests {
    use super::{DocumentLocale, Error, PageDimensions, PageGeometryError, PageMargins, PageSize};

    #[test]
    fn document_locale_normalizes_supported_codes() {
        assert_eq!(
            DocumentLocale::try_from_codes("PT", Some("br")),
            Ok(DocumentLocale {
                language: "pt".to_owned(),
                region: Some("BR".to_owned()),
            })
        );
        assert_eq!(
            DocumentLocale::try_from_codes("ENG", None),
            Ok(DocumentLocale {
                language: "eng".to_owned(),
                region: None,
            })
        );
    }

    #[test]
    fn document_locale_rejects_codes_typst_cannot_represent() {
        for language in ["e", "english", "en1"] {
            assert_eq!(
                DocumentLocale::try_from_codes(language, None),
                Err(Error::InvalidLanguage {
                    value: language.to_owned(),
                }),
            );
        }
        for region in ["B", "BRA", "01"] {
            assert_eq!(
                DocumentLocale::try_from_codes("en", Some(region)),
                Err(Error::InvalidRegion {
                    value: region.to_owned(),
                }),
            );
        }
    }

    #[test]
    fn custom_page_geometry_rejects_invalid_values() {
        for (width, height) in [
            (0.0, 1.0),
            (1.0, -1.0),
            (f64::INFINITY, 1.0),
            (1.0, f64::NAN),
        ] {
            assert_eq!(
                PageDimensions::try_new(width, height),
                Err(PageGeometryError::InvalidDimensions)
            );
        }
        assert_eq!(
            PageMargins::try_new(1.0, -1.0, 1.0, 1.0),
            Err(PageGeometryError::InvalidMargins)
        );
        assert_eq!(
            PageSize::try_custom(0.0, 1.0),
            Err(PageGeometryError::InvalidDimensions)
        );
    }
}
