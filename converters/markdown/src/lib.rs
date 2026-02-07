//! Markdown converter for `AsciiDoc` documents.
//!
//! This converter outputs Markdown format with support for both CommonMark
//! and GitHub Flavored Markdown (GFM) variants.
//!
//! # Example
//!
//! ```ignore
//! use acdc_converters_markdown::{Processor, MarkdownVariant};
//! use acdc_converters_core::{Converter, Options};
//!
//! let options = Options::default();
//! let processor = Processor::new(options, Default::default())
//!     .with_variant(MarkdownVariant::GitHubFlavored);
//! processor.convert(&document, Some(Path::new("doc.adoc")))?;
//! // Outputs: doc.md
//! ```
//!
//! # Markdown Variants
//!
//! ## CommonMark
//! - Standard Markdown specification (spec.commonmark.org)
//! - Basic features: headings, lists, links, images, code blocks, blockquotes
//! - No tables, task lists, or strikethrough
//!
//! ## GitHub Flavored Markdown (GFM)
//! - Extends CommonMark with GitHub-specific features
//! - Tables with alignment support
//! - Task lists (checkboxes)
//! - GitHub Alerts (admonitions using `> [!NOTE]` syntax)
//! - Footnotes with `[^1]` syntax
//! - Strikethrough (`~~text~~`)
//! - Autolinks for URLs and emails
//!
//! # Limitations
//!
//! AsciiDoc features that cannot be fully represented in Markdown:
//! - **Admonitions** (NOTE, TIP, etc.) - Native GitHub Alerts in GFM, blockquotes in CommonMark
//! - **Footnotes** - Native GFM syntax `[^1]`, HTML superscript in CommonMark
//! - **Tables** - Supported in GFM only, skipped in CommonMark with warning
//! - **Task lists** - Supported in GFM only, converted to regular lists in CommonMark
//! - **Include directives** - not supported (Markdown is single-file oriented)
//! - **Substitutions** - no control over text substitutions
//! - **Callouts** - code annotations not supported in standard Markdown
//! - **Table cell spanning** - GFM tables don't support rowspan/colspan
//! - **Video/audio embedding** - converted to links with warning
//! - **Complex tables** - GFM tables are simpler than AsciiDoc tables
//!
//! When unsupported features are encountered, the converter will:
//! - Emit a warning to stderr via `tracing::warn!`
//! - Provide a reasonable fallback (e.g., blockquote for admonitions)
//! - Preserve content as appropriate (e.g., raw text, URL/path)

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use acdc_converters_core::{Backend, Converter, Options, visitor::Visitor};
use acdc_parser::{Document, DocumentAttributes};

mod error;
mod markdown_visitor;

pub use error::Error;
pub use markdown_visitor::MarkdownVisitor;

/// Markdown variant/flavor for conversion.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MarkdownVariant {
    /// CommonMark specification (basic Markdown features only).
    CommonMark,
    /// GitHub Flavored Markdown (extends CommonMark with tables, task lists, etc.).
    #[default]
    GitHubFlavored,
}

/// Markdown converter processor.
#[derive(Clone, Debug)]
pub struct Processor {
    options: Options,
    document_attributes: DocumentAttributes,
    variant: MarkdownVariant,
}

impl Processor {
    /// Set the Markdown variant (CommonMark or GitHub Flavored).
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

impl Converter for Processor {
    type Error = Error;

    fn new(options: Options, document_attributes: DocumentAttributes) -> Self {
        Self {
            options,
            document_attributes,
            variant: MarkdownVariant::default(),
        }
    }

    fn options(&self) -> &Options {
        &self.options
    }

    fn document_attributes(&self) -> &DocumentAttributes {
        &self.document_attributes
    }

    fn derive_output_path(&self, input: &Path, _doc: &Document) -> Result<Option<PathBuf>, Error> {
        let md_path = input.with_extension("md");
        // Avoid overwriting the input file
        if md_path == input {
            return Err(Error::OutputPathSameAsInput(input.to_path_buf()));
        }
        Ok(Some(md_path))
    }

    fn write_to<W: Write>(
        &self,
        doc: &Document,
        writer: W,
        _source_file: Option<&Path>,
    ) -> Result<(), Self::Error> {
        let processor = Processor {
            document_attributes: doc.attributes.clone(),
            ..self.clone()
        };
        let mut visitor = MarkdownVisitor::new(writer, processor);
        visitor.visit_document(doc)?;
        Ok(())
    }

    fn backend(&self) -> Backend {
        Backend::Markdown
    }
}
