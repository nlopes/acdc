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
//! Both variants preserve section and block IDs, block and standalone inline
//! anchors, and IDs on formatted spans as portable HTML destinations for local
//! cross-references.
//! Parser-assigned section numbers are preserved, while `%notitle` hides only
//! the section heading. Tables of contents render as nested section links at
//! the configured automatic, preamble, or macro position.
//! Document headers retain subtitles, authors, revision details, and explicit
//! IDs. Block titles retain resolved caption labels and numbers, and quote and
//! verse credits remain visible.
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
    cell::Cell,
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    rc::Rc,
};

use acdc_converters_core::{
    BackendTraits, Converter, Diagnostics, Options, WarningSource, visitor::Visitor,
    xref::XrefGuard,
};
use acdc_parser::{
    BlockMetadata, Caption, CaptionKind, Document, DocumentAttributes, Reference, TocEntry,
};

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

/// Intrinsic traits for the Markdown backend.
const BACKEND_TRAITS: BackendTraits = BackendTraits::new("markdown", "markdown", "md", ".md");

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
    /// Cross-reference targets keyed by id, cloned from `Document::references`,
    /// so `<<id>>` resolves to its target's reference text.
    pub(crate) references: Rc<HashMap<&'a str, Reference<'a>>>,
    /// Keeps a cross-reference inside a resolved target's text from recursing.
    pub(crate) xref_guard: XrefGuard,
    pub(crate) toc_entries: Vec<TocEntry<'a>>,
    pub(crate) example_counter: Rc<Cell<u32>>,
    pub(crate) figure_counter: Rc<Cell<u32>>,
    pub(crate) listing_counter: Rc<Cell<u32>>,
    pub(crate) table_counter: Rc<Cell<u32>>,
    variant: MarkdownVariant,
}

impl Processor<'_> {
    /// Override the Markdown variant (`CommonMark` or GitHub Flavored).
    #[must_use]
    pub fn with_variant(mut self, variant: MarkdownVariant) -> Self {
        self.variant = variant;
        BACKEND_TRAITS.apply(&mut self.document_attributes, self.options.doctype());
        self
    }

    /// Get the current Markdown variant.
    #[must_use]
    pub fn variant(&self) -> MarkdownVariant {
        self.variant
    }

    pub(crate) fn caption_prefix(
        &self,
        metadata: &BlockMetadata<'_>,
        fallback: Option<CaptionKind>,
    ) -> Option<String> {
        let resolved = match (&metadata.caption, fallback) {
            (Some(caption), _) => caption.clone(),
            (None, Some(kind)) => Caption::resolve_owned(metadata, &self.document_attributes, kind),
            (None, None) => return None,
        };
        match resolved {
            Caption::Numbered {
                label,
                number,
                kind,
            } => {
                let number = number
                    .map_or_else(|| self.next_caption_number(kind), std::num::NonZeroU32::get);
                Some(format!("{label} {number}. "))
            }
            Caption::Custom(prefix) => Some(prefix.into_owned()),
            Caption::Unnumbered | _ => None,
        }
    }

    fn next_caption_number(&self, kind: CaptionKind) -> u32 {
        let counter = match kind {
            CaptionKind::Figure => &self.figure_counter,
            CaptionKind::Listing => &self.listing_counter,
            CaptionKind::Table => &self.table_counter,
            CaptionKind::Example | _ => &self.example_counter,
        };
        let number = counter.get() + 1;
        counter.set(number);
        number
    }
}

impl<'a> Converter<'a> for Processor<'a> {
    type Error = Error;

    fn new(options: Options, mut document_attributes: DocumentAttributes<'a>) -> Self {
        BACKEND_TRAITS.apply(&mut document_attributes, options.doctype());
        Self {
            options,
            document_attributes,
            references: Rc::new(HashMap::new()),
            xref_guard: XrefGuard::default(),
            toc_entries: Vec::new(),
            example_counter: Rc::new(Cell::new(0)),
            figure_counter: Rc::new(Cell::new(0)),
            listing_counter: Rc::new(Cell::new(0)),
            table_counter: Rc::new(Cell::new(0)),
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
        _doc: &Document<'_>,
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
        doc: &Document<'_>,
        writer: W,
        _source_file: Option<&Path>,
        _output_path: Option<&Path>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<(), Self::Error> {
        // Per-conversion processor borrows from `doc`; lifetime independent of `self`.
        let processor = Processor {
            options: self.options.clone(),
            document_attributes: doc.attributes.clone(),
            references: Rc::new(doc.references.clone()),
            xref_guard: XrefGuard::default(),
            toc_entries: doc.toc_entries.clone(),
            example_counter: Rc::new(Cell::new(doc.highest_caption_number(CaptionKind::Example))),
            figure_counter: Rc::new(Cell::new(doc.highest_caption_number(CaptionKind::Figure))),
            listing_counter: Rc::new(Cell::new(doc.highest_caption_number(CaptionKind::Listing))),
            table_counter: Rc::new(Cell::new(doc.highest_caption_number(CaptionKind::Table))),
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
        assert_eq!(
            processor
                .document_attributes()
                .get_string("backend")
                .as_deref(),
            Some("markdown")
        );
        assert_eq!(
            processor
                .document_attributes()
                .get_string("filetype")
                .as_deref(),
            Some("md")
        );
    }

    #[test]
    fn with_variant_switches_to_commonmark() {
        let processor = Processor::new(Options::default(), DocumentAttributes::default())
            .with_variant(MarkdownVariant::CommonMark);
        assert_eq!(processor.variant(), MarkdownVariant::CommonMark);
    }

    #[test]
    fn media_targets_honor_imagesdir_and_encode_spaces() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            ":imagesdir: media library\n\nimage::poster file.png[]\n\nimage::already%20encoded.png[]\n\nInline image:inline poster.png[].\n\naudio::clips/demo track.mp3[]\n\nvideo::clips/demo clip.mp4[]\n",
            &acdc_parser::Options::default(),
        )?;
        let processor = Processor::new(Options::default(), parsed.document().attributes.clone());
        let source = processor.warning_source();
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let mut output = Vec::new();

        processor.write_to(parsed.document(), &mut output, None, None, &mut diagnostics)?;

        let markdown = String::from_utf8(output)?;
        for target in [
            "](media%20library/poster%20file.png)",
            "](media%20library/already%20encoded.png)",
            "](media%20library/inline%20poster.png)",
            "[Audio: media%20library/clips/demo%20track.mp3](media%20library/clips/demo%20track.mp3)",
            "[Video: media%20library/clips/demo%20clip.mp4](media%20library/clips/demo%20clip.mp4)",
        ] {
            assert!(markdown.contains(target), "missing {target}: {markdown}");
        }
        assert!(
            !markdown.contains("%2520"),
            "double-encoded target: {markdown}"
        );
        Ok(())
    }
}
