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
    /// Standard paper size used for every document page.
    pub page: PageSize,
    /// Portrait or landscape layout used for every document page.
    pub page_layout: PageLayout,
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
            plain: false,
            brand_fonts: false,
            running_header_title: None,
            logo: None,
            watermark: None,
            watermark_timestamp: None,
        }
    }
}

/// A supported page size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSize {
    A3,
    A4,
    A5,
    Executive,
    Legal,
    Letter,
    Tabloid,
}

/// A supported document page layout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PageLayout {
    #[default]
    Portrait,
    Landscape,
}

impl PageSize {
    /// Returns the page width after applying `layout`, in PDF points.
    #[must_use]
    pub const fn width_points(self, layout: PageLayout) -> f64 {
        let (width, height) = match self {
            Self::A3 => (841.89, 1190.551),
            Self::A4 => (595.276, 841.89),
            Self::A5 => (419.528, 595.276),
            Self::Executive => (522.0, 756.0),
            Self::Legal => (612.0, 1008.0),
            Self::Letter => (612.0, 792.0),
            Self::Tabloid => (792.0, 1224.0),
        };
        match layout {
            PageLayout::Portrait => width,
            PageLayout::Landscape => height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DocumentLocale, Error};

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
}
