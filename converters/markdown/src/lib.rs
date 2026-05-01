//! Markdown converter for `AsciiDoc` documents.
//!
//! This converter outputs Markdown format with support for both `CommonMark`
//! and GitHub Flavored Markdown (GFM) variants.
//!
//! # Example
//!
//! ```ignore
//! use acdc_converters_markdown::{MarkdownVariant, Processor};
//! use acdc_converters_core::{Converter, Options};
//!
//! let options = Options::default();
//! let processor = Processor::new(options, Default::default())
//!     .with_variant(MarkdownVariant::CommonMark);
//! processor.convert(&document, Some(Path::new("doc.adoc")))?;
//! // Outputs: doc.md
//! ```
//!
//! # Markdown Variants
//!
//! ## `CommonMark`
//! - Standard Markdown specification (spec.commonmark.org)
//! - Basic features: headings, lists, links, images, code blocks, blockquotes
//! - No tables, task lists, or strikethrough
//!
//! ## GitHub Flavored Markdown (GFM)
//! - Extends `CommonMark` with GitHub-specific features
//! - Tables with alignment support
//! - Task lists (checkboxes)
//! - GitHub Alerts (admonitions using `> [!NOTE]` syntax)
//! - Footnotes with `[^1]` syntax
//! - Strikethrough (`~~text~~`)
//! - Autolinks for URLs and emails
//!
//! # Limitations
//!
//! `AsciiDoc` features that cannot be fully represented in Markdown:
//! - **Admonitions** (NOTE, TIP, etc.) - Native GitHub Alerts in GFM, blockquotes in `CommonMark`
//! - **Footnotes** - Native GFM syntax `[^1]`, HTML superscript in `CommonMark`
//! - **Tables** - Supported in GFM only, skipped in `CommonMark` with warning
//! - **Task lists** - Supported in GFM only, converted to regular lists in `CommonMark`
//! - **Include directives** - not supported (Markdown is single-file oriented)
//! - **Substitutions** - no control over text substitutions
//! - **Callouts** - code annotations not supported in standard Markdown
//! - **Table cell spanning** - GFM tables don't support rowspan/colspan
//! - **Video/audio embedding** - converted to links with warning
//! - **Complex tables** - GFM tables are simpler than `AsciiDoc` tables
//!
//! When unsupported features are encountered, the converter will:
//! - Collect a structured converter warning
//! - Provide a reasonable fallback (e.g., blockquote for admonitions)
//! - Preserve content as appropriate (e.g., raw text, URL/path)

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use acdc_converters_core::{Converter, Diagnostics, Options, WarningSource, visitor::Visitor};
use acdc_parser::{Document, DocumentAttributes};

mod error;
mod markdown_visitor;

pub use error::Error;
pub use markdown_visitor::MarkdownVisitor;

/// Markdown output flavour, owned by the markdown converter.
///
/// Pick a variant via [`Processor::with_variant`]; [`Processor::new`]
/// defaults to [`MarkdownVariant::GitHubFlavored`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MarkdownVariant {
    /// `CommonMark` Markdown (basic features only — no tables/task lists).
    CommonMark,
    /// GitHub Flavored Markdown (extends `CommonMark` with tables, task
    /// lists, alerts, footnotes, strikethrough, autolinks).
    #[default]
    GitHubFlavored,
}

impl std::str::FromStr for MarkdownVariant {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_ref() {
            "commonmark" | "cm" => Ok(Self::CommonMark),
            "gfm" | "github-flavored" | "github" => Ok(Self::GitHubFlavored),
            _ => Err(format!(
                "invalid markdown variant: '{s}', expected: commonmark, gfm"
            )),
        }
    }
}

impl MarkdownVariant {
    /// Lower-case static name for this variant (`"commonmark"` / `"gfm"`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CommonMark => "commonmark",
            Self::GitHubFlavored => "gfm",
        }
    }
}

impl std::fmt::Display for MarkdownVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Markdown converter processor.
#[derive(Clone, Debug)]
pub struct Processor<'a> {
    options: Options,
    document_attributes: DocumentAttributes<'a>,
    variant: MarkdownVariant,
}

impl Processor<'_> {
    /// Override the Markdown variant (`CommonMark` or GitHub Flavored).
    #[must_use]
    pub fn with_variant(mut self, variant: MarkdownVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Get the current Markdown variant.
    #[must_use]
    pub fn variant(&self) -> MarkdownVariant {
        self.variant
    }
}

impl<'a> Converter<'a> for Processor<'a> {
    type Error = Error;

    fn new(options: Options, document_attributes: DocumentAttributes<'a>) -> Self {
        Self {
            options,
            document_attributes,
            variant: MarkdownVariant::default(),
        }
    }

    fn options(&self) -> &Options {
        &self.options
    }

    fn document_attributes(&self) -> &DocumentAttributes<'a> {
        &self.document_attributes
    }

    fn derive_output_path(
        &self,
        input: &Path,
        _doc: &Document<'a>,
    ) -> Result<Option<PathBuf>, Error> {
        let md_path = input.with_extension("md");
        // Avoid overwriting the input file
        if md_path == input {
            return Err(Error::OutputPathSameAsInput(input.to_path_buf()));
        }
        Ok(Some(md_path))
    }

    fn write_to<W: Write>(
        &self,
        doc: &Document<'a>,
        writer: W,
        _source_file: Option<&Path>,
        _output_path: Option<&Path>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<(), Self::Error> {
        let processor = Processor {
            options: self.options.clone(),
            document_attributes: doc.attributes.clone(),
            variant: self.variant,
        };
        let mut visitor = MarkdownVisitor::new(writer, processor, diagnostics.reborrow());
        visitor.visit_document(doc)
    }

    fn name(&self) -> &'static str {
        "markdown"
    }

    fn warning_source(&self) -> WarningSource {
        WarningSource::new("markdown").with_variant(self.variant.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_defaults_to_gfm() {
        let processor = Processor::new(Options::default(), DocumentAttributes::default());
        assert_eq!(processor.variant(), MarkdownVariant::GitHubFlavored);
    }

    #[test]
    fn with_variant_switches_to_commonmark() {
        let processor = Processor::new(Options::default(), DocumentAttributes::default())
            .with_variant(MarkdownVariant::CommonMark);
        assert_eq!(processor.variant(), MarkdownVariant::CommonMark);
    }
}
