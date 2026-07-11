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

/// Document-level options that shape the generated markup.
#[derive(Debug, Clone)]
pub struct EmitOptions {
    pub page: PageSize,
    /// Strip branding chrome (page background, header, footer).
    pub plain: bool,
    /// Emit a table of contents from the headings.
    pub toc: bool,
    /// Whether brand fonts are available at render time. When set, the brand
    /// family is named first in each font stack; otherwise only the bundled
    /// fallbacks are named (so Typst is never asked for an absent font).
    pub brand_fonts: bool,
    /// Document title, shown in the branded header when set.
    pub title: Option<String>,
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
            page: PageSize::A4,
            plain: false,
            toc: false,
            brand_fonts: false,
            title: None,
            logo: None,
            watermark: None,
            watermark_timestamp: None,
        }
    }
}

/// A supported page size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSize {
    A4,
    Letter,
}
