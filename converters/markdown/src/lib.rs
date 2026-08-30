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
//! - No native tables, task-list, or strikethrough syntax
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
//! Styled paragraphs use blockquotes or fenced code as appropriate. Raw
//! passthrough blocks remain available to HTML-capable Markdown renderers, and
//! source callouts retain readable `(n)` markers, bold numbered explanation
//! labels, and attached blocks.
//! Inline UI macros, passthroughs, STEM expressions, and roles retain readable
//! content through native Markdown, embedded HTML, or inline-code fallbacks.
//! Link fallback text honors `hide-uri-scheme`, bracketed email autolinks stay
//! visible, and link destinations escape Markdown-sensitive characters.
//! Ordered lists retain reversed numbering, and ordered and unordered lists
//! retain markerless presentation. GFM uses native task-list syntax, while
//! `CommonMark` retains checklist state as visible text. Horizontal and Q&A
//! description lists have distinct layouts. Bibliography entries retain their
//! anchors and visible bracketed labels.
//! GFM tables retain column-level horizontal alignment and every source row.
//! Headerless tables use an empty header, footers become final body rows, and
//! unsupported spans, styles, widths, local alignment, and nested blocks use
//! content-preserving fallbacks with structured warnings.
//! Images retain alternative text, titles, dimensions, and links. Video
//! posters render as static images, and every audio or video source remains
//! available as a labeled link, with one playback warning per document.
//! With `:acdc-index:` and a final `[index]` section, index terms produce an
//! alphabetized catalog with occurrence links, hierarchy, and `see` /
//! `see-also` relationships. This extension uses the same opt-in policy as the
//! HTML converter.
//!
//! # Limitations
//!
//! `AsciiDoc` features that cannot be fully represented in Markdown:
//! - **Admonitions** (NOTE, TIP, etc.) - Native GitHub Alerts in GFM, blockquotes in `CommonMark`
//! - **Footnotes** - Native GFM syntax `[^1]`, linked superscripts and a
//!   readable endnote list in `CommonMark`
//! - **Tables** - GFM preserves content through readable fallbacks;
//!   `CommonMark` skips tables with a warning
//! - **Task lists** - Native in GFM; `CommonMark` uses visible literal markers
//! - **Include directives** - not supported (Markdown is single-file oriented)
//! - **Substitutions** - no control over text substitutions
//! - **Source line presentation** - line numbers, selected-line highlighting,
//!   and PHP mixed-mode highlighting retain code and produce warnings
//! - **Video/audio embedding** - video posters render as images and every
//!   source as a labeled link, with one warning per document
//! - **Complex tables** - GFM cannot preserve spans, structural footers,
//!   widths, local alignment, or nested block structure exactly
//! - **Inline STEM** - preserved as inline code with a warning
//! - **Inline roles and passthroughs** - use embedded HTML where Markdown has
//!   no equivalent, so rendering depends on HTML support and sanitization
//!
//! When unsupported features are encountered, the converter will:
//! - Collect a structured converter warning
//! - Provide a reasonable fallback (e.g., blockquote for admonitions)
//! - Preserve content as appropriate (e.g., raw text, URL/path)
//!
//! Document-wide fallback warnings are emitted once per capability. Any
//! warnings about individual resources remain distinct so each failed resource
//! can be identified.

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    rc::Rc,
};

use acdc_converters_core::{
    BackendTraits, Converter, Diagnostics, Options, WarningSource, section::last_section_has_style,
    visitor::Visitor, xref::XrefGuard,
};
use acdc_parser::{
    AttributeValue, BlockMetadata, Caption, CaptionKind, Document, DocumentAttributes, Reference,
    TocEntry,
};

mod error;
mod index;
mod markdown_visitor;

pub use error::Error;
pub use markdown_visitor::MarkdownVisitor;

/// Markdown output flavour, owned by the markdown converter.
///
/// Pick a variant via [`Processor::with_variant`]; [`Processor::new`]
/// defaults to [`MarkdownVariant::GitHubFlavored`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MarkdownVariant {
    /// `CommonMark` Markdown (basic features only; no native tables or task lists).
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

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct IndexTermLabel {
    pub(crate) plain: String,
    pub(crate) rendered: String,
}

#[derive(Clone, Debug)]
pub(crate) struct IndexTermEntry {
    pub(crate) primary: IndexTermLabel,
    pub(crate) secondary: Option<IndexTermLabel>,
    pub(crate) tertiary: Option<IndexTermLabel>,
    pub(crate) relationship: IndexCatalogRelationship,
    pub(crate) anchor_id: String,
    pub(crate) section_title: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) enum IndexCatalogRelationship {
    None,
    See(IndexTermLabel),
    SeeAlso(Vec<IndexTermLabel>),
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
    index_term_counter: Rc<Cell<usize>>,
    index_entries: Rc<RefCell<Vec<IndexTermEntry>>>,
    generate_index: bool,
    warned_fallbacks: Rc<RefCell<HashSet<&'static str>>>,
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

    #[must_use]
    pub(crate) fn generate_index(&self) -> bool {
        self.generate_index
    }

    #[must_use]
    pub(crate) fn index_entries(&self) -> &Rc<RefCell<Vec<IndexTermEntry>>> {
        &self.index_entries
    }

    #[must_use]
    pub(crate) fn add_index_entry(&self, mut entry: IndexTermEntry) -> String {
        let count = self.index_term_counter.get();
        self.index_term_counter.set(count + 1);
        let anchor_id = format!("_indexterm_{count}");
        entry.anchor_id.clone_from(&anchor_id);
        self.index_entries.borrow_mut().push(entry);
        anchor_id
    }

    pub(crate) fn mark_fallback(&self, key: &'static str) -> bool {
        self.warned_fallbacks.borrow_mut().insert(key)
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
            index_term_counter: Rc::new(Cell::new(0)),
            index_entries: Rc::new(RefCell::new(Vec::new())),
            generate_index: false,
            warned_fallbacks: Rc::new(RefCell::new(HashSet::new())),
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
            index_term_counter: Rc::new(Cell::new(0)),
            index_entries: Rc::new(RefCell::new(Vec::new())),
            generate_index: index_generation_enabled(&doc.attributes)
                && last_section_has_style(&doc.blocks, "index"),
            warned_fallbacks: Rc::new(RefCell::new(HashSet::new())),
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

fn index_generation_enabled(attributes: &DocumentAttributes<'_>) -> bool {
    attributes
        .get("acdc-index")
        .is_some_and(|value| !matches!(value, AttributeValue::Bool(false) | AttributeValue::None))
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
    fn media_targets_honor_imagesdir_and_escape_destinations()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(
            ":imagesdir: media (library)\n\nimage::poster file.png[]\n\nimage::already%20encoded.png[]\n\nInline image:inline poster.png[].\n\naudio::clips/demo track.mp3[]\n\nvideo::clips/demo clip.mp4[]\n",
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
            "](media%20\\(library\\)/poster%20file.png)",
            "](media%20\\(library\\)/already%20encoded.png)",
            "](media%20\\(library\\)/inline%20poster.png)",
            "[Audio: demo track.mp3](media%20\\(library\\)/clips/demo%20track.mp3)",
            "[Video: demo clip.mp4](media%20\\(library\\)/clips/demo%20clip.mp4)",
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
