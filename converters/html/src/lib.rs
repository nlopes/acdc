use std::{
    cell::{Cell, RefCell},
    io::Write,
    path::{Path, PathBuf},
    rc::Rc,
};

use acdc_converters_core::{Backend, Converter, Options, visitor::Visitor};
use acdc_parser::{AttributeValue, Block, Document, DocumentAttributes, IndexTermKind, TocEntry};

mod admonition;
mod audio;
mod constants;
mod delimited;
mod docinfo;
mod document;
mod error;
mod html_visitor;
mod icon;
mod image;
mod image_helpers;
mod index;
mod inlines;
mod list;
mod paragraph;
mod section;
mod syntax;
mod table;
mod toc;
mod video;

pub(crate) use acdc_converters_core::section::{
    AppendixTracker, PartNumberTracker, SectionNumberTracker,
};
pub use error::Error;
pub use html_visitor::HtmlVisitor;

/// Controls the HTML output style.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HtmlVariant {
    /// Standard asciidoctor-compatible HTML (div soup).
    #[default]
    Standard,
    /// Semantic HTML5 output using section, aside, figure, ARIA roles, etc.
    Semantic,
}

/// An entry in the index catalog, collected during document traversal.
#[derive(Clone, Debug)]
pub struct IndexTermEntry {
    /// The index term kind (Flow or Concealed with hierarchy)
    pub kind: IndexTermKind,
    /// Anchor ID for linking back to the term's location
    pub anchor_id: String,
}

#[derive(Clone, Debug)]
pub struct Processor {
    options: Options,
    document_attributes: DocumentAttributes,
    toc_entries: Vec<TocEntry>,
    /// Shared counter for auto-numbering example blocks.
    /// Uses Rc<Cell<>> so all clones share the same counter.
    example_counter: Rc<Cell<usize>>,
    /// Shared counter for auto-numbering table blocks.
    /// Uses Rc<Cell<>> so all clones share the same counter.
    table_counter: Rc<Cell<usize>>,
    /// Shared counter for auto-numbering figure blocks.
    /// Uses Rc<Cell<>> so all clones share the same counter.
    figure_counter: Rc<Cell<usize>>,
    /// Shared counter for auto-numbering listing blocks.
    /// Uses Rc<Cell<>> so all clones share the same counter.
    /// Only used when listing-caption attribute is set.
    listing_counter: Rc<Cell<usize>>,
    /// Shared counter for generating unique index term anchor IDs.
    index_term_counter: Rc<Cell<usize>>,
    /// Collected index term entries for rendering in the index catalog.
    /// Uses `Rc<RefCell<>>` so all clones can add entries during traversal.
    index_entries: Rc<RefCell<Vec<IndexTermEntry>>>,
    /// Whether the document's last section has the `[index]` style.
    /// Index sections are only rendered if they are the last section.
    has_valid_index_section: bool,
    /// Section number tracker for `:sectnums:` support.
    section_number_tracker: SectionNumberTracker,
    /// Part number tracker for `:partnums:` support in book doctype.
    part_number_tracker: PartNumberTracker,
    /// Appendix tracker for `[appendix]` style on level-0 sections.
    appendix_tracker: AppendixTracker,
    /// HTML output variant (Standard or Semantic).
    variant: HtmlVariant,
}

impl Processor {
    /// Get a reference to the document attributes
    #[must_use]
    pub fn document_attributes(&self) -> &DocumentAttributes {
        &self.document_attributes
    }

    /// Get a reference to the TOC entries
    #[must_use]
    pub fn toc_entries(&self) -> &[TocEntry] {
        &self.toc_entries
    }

    /// Get a reference to the collected index entries
    #[must_use]
    pub fn index_entries(&self) -> &Rc<RefCell<Vec<IndexTermEntry>>> {
        &self.index_entries
    }

    /// Check if the document has a valid index section (last section with `[index]` style).
    #[must_use]
    pub fn has_valid_index_section(&self) -> bool {
        self.has_valid_index_section
    }

    /// Get the HTML output variant.
    #[must_use]
    pub fn variant(&self) -> HtmlVariant {
        self.variant
    }

    /// Check if font icons mode is enabled (`:icons: font`).
    #[must_use]
    pub(crate) fn is_font_icons_mode(&self) -> bool {
        self.document_attributes
            .get("icons")
            .is_some_and(|v| v.to_string() == "font")
    }

    /// Get a reference to the section number tracker
    #[must_use]
    pub(crate) fn section_number_tracker(&self) -> &SectionNumberTracker {
        &self.section_number_tracker
    }

    /// Get a reference to the part number tracker
    #[must_use]
    pub(crate) fn part_number_tracker(&self) -> &PartNumberTracker {
        &self.part_number_tracker
    }

    /// Get a reference to the appendix tracker
    #[must_use]
    pub(crate) fn appendix_tracker(&self) -> &AppendixTracker {
        &self.appendix_tracker
    }

    /// Generate a caption prefix based on document attributes.
    ///
    /// Returns the caption prefix string. If captions are disabled via `:X-caption!:`,
    /// returns an empty string. Otherwise increments the counter and returns
    /// "Caption N. " format.
    #[must_use]
    pub(crate) fn caption_prefix(
        &self,
        attribute_name: &str,
        counter: &Rc<Cell<usize>>,
        default_text: &str,
    ) -> String {
        match self.document_attributes.get(attribute_name) {
            Some(AttributeValue::Bool(false)) => {
                // Disabled via :X-caption!:
                String::new()
            }
            Some(AttributeValue::String(s)) => {
                let count = counter.get() + 1;
                counter.set(count);
                let caption = s.trim_matches('"');
                format!("{caption} {count}. ")
            }
            _ => {
                let count = counter.get() + 1;
                counter.set(count);
                format!("{default_text} {count}. ")
            }
        }
    }

    /// Generate a unique anchor ID for an index term and collect the entry.
    #[must_use]
    pub fn add_index_entry(&self, kind: IndexTermKind) -> String {
        let count = self.index_term_counter.get();
        self.index_term_counter.set(count + 1);
        let anchor_id = format!("_indexterm_{count}");

        self.index_entries.borrow_mut().push(IndexTermEntry {
            kind,
            anchor_id: anchor_id.clone(),
        });

        anchor_id
    }

    /// Convert a document to HTML, writing to the provided writer.
    ///
    /// # Errors
    ///
    /// Returns an error if document conversion or writing fails.
    pub fn convert_to_writer<W: Write>(
        &self,
        doc: &Document,
        writer: W,
        options: &RenderOptions,
    ) -> Result<(), Error> {
        let section_number_tracker = SectionNumberTracker::new(&doc.attributes);
        let part_number_tracker =
            PartNumberTracker::new(&doc.attributes, section_number_tracker.clone());
        let appendix_tracker =
            AppendixTracker::new(&doc.attributes, section_number_tracker.clone());
        let processor = Processor {
            toc_entries: doc.toc_entries.clone(),
            document_attributes: doc.attributes.clone(),
            has_valid_index_section: Self::last_section_is_index(&doc.blocks),
            section_number_tracker,
            part_number_tracker,
            appendix_tracker,
            ..self.clone()
        };
        let mut visitor = HtmlVisitor::new(writer, processor, options.clone());
        visitor.visit_document(doc)?;
        Ok(())
    }

    /// Check if the last section in the document has the `[index]` style.
    fn last_section_is_index(blocks: &[Block]) -> bool {
        // Find the last section in the block list
        let last_section = blocks.iter().rev().find_map(|block| {
            if let Block::Section(section) = block {
                Some(section)
            } else {
                None
            }
        });

        // Check if it has the index style
        last_section.is_some_and(|section| {
            section
                .metadata
                .style
                .as_ref()
                .is_some_and(|s| s == "index")
        })
    }

    /// Convert a document to an HTML string.
    ///
    /// Use `RenderOptions::embedded` to control whether output includes the full
    /// document frame (DOCTYPE, html, head, body) or just embeddable content.
    ///
    /// # Errors
    ///
    /// Returns an error if document conversion fails.
    pub fn convert_to_string(
        &self,
        doc: &Document,
        options: &RenderOptions,
    ) -> Result<String, Error> {
        let mut output = Vec::new();
        self.convert_to_writer(doc, &mut output, options)?;
        Ok(String::from_utf8(output)?)
    }
}

#[derive(Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct RenderOptions {
    pub last_updated: Option<chrono::DateTime<chrono::Utc>>,
    pub inlines_basic: bool,
    pub inlines_verbatim: bool,
    /// When true, output embeddable document (no DOCTYPE, html, head, body wrapper).
    /// Follows Asciidoctor's embedded mode behavior - excludes header/footer frame
    /// but keeps body content structure including wrapper divs.
    pub embedded: bool,
    /// When true, render inline elements for TOC context (no nested links).
    /// TOC entries are already wrapped in `<a href="#section">`, so nested `<a>` tags
    /// would be invalid HTML. This mode renders link-producing macros as text only
    /// and skips decorative elements like images and icons.
    pub toc_mode: bool,
    /// Directory of the source document, used to resolve relative `stylesdir` paths.
    pub source_dir: Option<PathBuf>,
    /// Stem of the source document filename (e.g., `"mydoc"` for `mydoc.adoc`),
    /// used to locate private docinfo files like `mydoc-docinfo.html`.
    pub docname: Option<String>,
}

pub(crate) const COPYCSS_DEFAULT: &str = "";
pub(crate) const STYLESDIR_DEFAULT: &str = ".";
pub(crate) const STYLESHEET_DEFAULT: &str = "";
/// Default filename for the syntect syntax highlighting stylesheet (class-based mode).
/// Analogous to asciidoctor's `asciidoctor-coderay.css` / `asciidoctor-pygments.css`.
pub(crate) const SYNTECT_STYLESHEET: &str = "acdc-syntect.css";
// NOTE: If you change the values below, you need to also change them in `load_css`
pub(crate) const STYLESHEET_LIGHT_MODE: &str = "asciidoctor-light-mode.css";
pub(crate) const STYLESHEET_DARK_MODE: &str = "asciidoctor-dark-mode.css";
pub(crate) const STYLESHEET_HTML5S_LIGHT_MODE: &str = "html5s-light-mode.css";
pub(crate) const STYLESHEET_HTML5S_DARK_MODE: &str = "html5s-dark-mode.css";
pub(crate) const WEBFONTS_DEFAULT: &str = "";

pub(crate) fn load_css(dark_mode: bool, variant: HtmlVariant) -> &'static str {
    match (variant, dark_mode) {
        (HtmlVariant::Semantic, true) => include_str!("../static/html5s-dark-mode.css"),
        (HtmlVariant::Semantic, false) => include_str!("../static/html5s-light-mode.css"),
        (HtmlVariant::Standard, true) => include_str!("../static/asciidoctor-dark-mode.css"),
        (HtmlVariant::Standard, false) => include_str!("../static/asciidoctor-light-mode.css"),
    }
}

/// Resolve the syntax highlighting theme name and mode from document attributes.
///
/// - `:syntect-style:` overrides the theme (falls back to light/dark default).
/// - `:syntect-css: class` switches to CSS-class mode (default is inline).
pub(crate) fn resolve_highlight_settings(processor: &Processor) -> (String, syntax::HighlightMode) {
    let dark_mode = processor
        .document_attributes
        .get("dark-mode")
        .is_some_and(|v| !matches!(v, AttributeValue::Bool(false) | AttributeValue::None));

    let theme_name = processor
        .document_attributes
        .get("syntect-style")
        .and_then(|v| match v {
            AttributeValue::String(s) if !s.is_empty() => Some(s.clone()),
            AttributeValue::String(_) | AttributeValue::Bool(_) | AttributeValue::None | _ => None,
        })
        .unwrap_or_else(|| {
            if dark_mode {
                syntax::DEFAULT_THEME_DARK.to_string()
            } else {
                syntax::DEFAULT_THEME_LIGHT.to_string()
            }
        });

    let mode = if processor
        .document_attributes
        .get("syntect-css")
        .is_some_and(|v| matches!(v, AttributeValue::String(s) if s == "class"))
    {
        syntax::HighlightMode::Class
    } else {
        syntax::HighlightMode::Inline
    };

    (theme_name, mode)
}

impl Converter for Processor {
    type Error = Error;

    fn document_attributes_defaults() -> DocumentAttributes {
        let mut attrs = DocumentAttributes::default();
        // HTML-specific defaults from asciidoctor spec
        attrs.insert(
            "copycss".into(),
            AttributeValue::String(COPYCSS_DEFAULT.into()),
        );
        attrs.insert(
            "stylesdir".into(),
            AttributeValue::String(STYLESDIR_DEFAULT.into()),
        );
        attrs.insert(
            "stylesheet".into(),
            AttributeValue::String(STYLESHEET_DEFAULT.into()),
        );
        // Additional CSS styling attributes
        attrs.insert(
            "webfonts".into(),
            AttributeValue::String(WEBFONTS_DEFAULT.into()),
        );
        attrs
    }

    fn new(options: Options, document_attributes: DocumentAttributes) -> Self {
        let backend = options.backend();
        let variant = match backend {
            Backend::Html5s => HtmlVariant::Semantic,
            Backend::Html => HtmlVariant::Standard,
            Backend::Manpage | Backend::Markdown | Backend::Terminal => {
                tracing::error!(%backend, "backend not appropriate for this processor, assuming user meant html");
                HtmlVariant::Standard
            }
        };
        Self::new_with_variant(options, document_attributes, variant)
    }

    fn options(&self) -> &Options {
        &self.options
    }

    fn document_attributes(&self) -> &DocumentAttributes {
        &self.document_attributes
    }

    fn derive_output_path(&self, input: &Path, _doc: &Document) -> Result<Option<PathBuf>, Error> {
        let html_path = input.with_extension("html");
        // Avoid overwriting the input file
        if html_path == input {
            return Err(Error::OutputPathSameAsInput(input.to_path_buf()));
        }
        Ok(Some(html_path))
    }

    fn write_to<W: Write>(
        &self,
        doc: &Document,
        writer: W,
        source_file: Option<&Path>,
    ) -> Result<(), Self::Error> {
        let render_options = RenderOptions {
            last_updated: source_file.and_then(|f| {
                std::fs::metadata(f)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(chrono::DateTime::from)
            }),
            embedded: self.options.embedded(),
            source_dir: source_file.and_then(|f| f.parent().map(Path::to_path_buf)),
            docname: source_file
                .and_then(|f| f.file_stem())
                .and_then(|s| s.to_str())
                .map(String::from),
            ..RenderOptions::default()
        };
        self.convert_to_writer(doc, writer, &render_options)
    }

    fn after_write(&self, doc: &Document, output_path: &Path) {
        if self.options.embedded() {
            return;
        }
        self.handle_copycss(doc, output_path);
        self.handle_copy_syntax_css(doc, output_path);
    }

    fn backend(&self) -> Backend {
        match self.variant {
            HtmlVariant::Semantic => Backend::Html5s,
            HtmlVariant::Standard => Backend::Html,
        }
    }
}

impl Processor {
    /// Create a processor with a specific HTML variant.
    ///
    /// Useful for tests and callers that construct `Processor` directly
    /// without going through the `Converter` trait.
    #[must_use]
    pub fn new_with_variant(
        options: Options,
        document_attributes: DocumentAttributes,
        variant: HtmlVariant,
    ) -> Self {
        let mut document_attributes = document_attributes;
        for (name, value) in <Self as Converter>::document_attributes_defaults().iter() {
            document_attributes.insert(name.clone(), value.clone());
        }

        let section_number_tracker = SectionNumberTracker::new(&document_attributes);
        let part_number_tracker =
            PartNumberTracker::new(&document_attributes, section_number_tracker.clone());
        let appendix_tracker =
            AppendixTracker::new(&document_attributes, section_number_tracker.clone());

        Self {
            options,
            document_attributes,
            toc_entries: vec![],
            example_counter: Rc::new(Cell::new(0)),
            table_counter: Rc::new(Cell::new(0)),
            figure_counter: Rc::new(Cell::new(0)),
            listing_counter: Rc::new(Cell::new(0)),
            index_term_counter: Rc::new(Cell::new(0)),
            index_entries: Rc::new(RefCell::new(Vec::new())),
            has_valid_index_section: false,
            section_number_tracker,
            part_number_tracker,
            appendix_tracker,
            variant,
        }
    }

    /// Return the appropriate default stylesheet filename for this processor's variant.
    fn default_stylesheet_name(&self, is_dark: bool) -> &'static str {
        match (self.variant, is_dark) {
            (HtmlVariant::Semantic, true) => STYLESHEET_HTML5S_DARK_MODE,
            (HtmlVariant::Semantic, false) => STYLESHEET_HTML5S_LIGHT_MODE,
            (HtmlVariant::Standard, true) => STYLESHEET_DARK_MODE,
            (HtmlVariant::Standard, false) => STYLESHEET_LIGHT_MODE,
        }
    }

    /// Handle copying CSS if linkcss and copycss are set.
    ///
    /// When the default (built-in) stylesheet is active, the CSS content is written
    /// directly to disk since there is no source file to copy.  When `copycss` has a
    /// non-empty string value it is used as the source path override (the file to read
    /// from), decoupling the source location from the output reference.
    fn handle_copycss(&self, doc: &acdc_parser::Document, html_path: &std::path::Path) {
        // No-stylesheet mode — nothing to copy
        let stylesheet_disabled = doc
            .attributes
            .get("stylesheet")
            .is_some_and(|v| matches!(v, AttributeValue::Bool(false)));
        if stylesheet_disabled {
            return;
        }

        let linkcss = doc.attributes.get("linkcss").is_some();
        if !linkcss {
            return;
        }

        let should_copy = doc.attributes.contains_key("copycss");
        tracing::debug!("linkcss={linkcss}, copycss exists={should_copy}");

        if !should_copy {
            return;
        }

        let is_dark = doc
            .attributes
            .get("dark-mode")
            .is_some_and(|v| !matches!(v, AttributeValue::Bool(false) | AttributeValue::None));
        let default_filename = self.default_stylesheet_name(is_dark);

        // Determine whether the default (built-in) stylesheet is in use
        let using_default = doc
            .attributes
            .get("stylesheet")
            .is_none_or(|v| v.to_string().is_empty());

        let stylesheet = doc
            .attributes
            .get("stylesheet")
            .and_then(|v| {
                let s = v.to_string();
                if s.is_empty() { None } else { Some(s) }
            })
            .unwrap_or_else(|| default_filename.into());

        let output_dir = html_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let dest_path = output_dir.join(&stylesheet);

        // Check if copycss has a non-empty value (source path override)
        let copycss_source = doc.attributes.get("copycss").and_then(|v| {
            let s = v.to_string();
            if s.is_empty() { None } else { Some(s) }
        });

        if using_default && copycss_source.is_none() {
            // Write built-in CSS content to disk (no source file exists)
            let css_content = load_css(is_dark, self.variant);
            if let Err(e) = std::fs::write(&dest_path, css_content) {
                tracing::warn!(
                    "Failed to write built-in stylesheet to {}: {e}",
                    dest_path.display(),
                );
            } else {
                tracing::debug!("Wrote built-in stylesheet to {}", dest_path.display());
            }
        } else {
            // Custom stylesheet or copycss source override — copy file
            let stylesdir = doc
                .attributes
                .get("stylesdir")
                .map_or_else(|| STYLESDIR_DEFAULT.into(), ToString::to_string);

            let source_path = if let Some(ref custom_source) = copycss_source {
                std::path::PathBuf::from(custom_source)
            } else if stylesdir.is_empty() || stylesdir == STYLESDIR_DEFAULT {
                std::path::Path::new(&stylesheet).to_path_buf()
            } else {
                std::path::Path::new(&stylesdir).join(&stylesheet)
            };

            if source_path != dest_path && source_path.exists() {
                if let Err(e) = std::fs::copy(&source_path, &dest_path) {
                    tracing::warn!(
                        "Failed to copy stylesheet from {} to {}: {e}",
                        source_path.display(),
                        dest_path.display(),
                    );
                } else {
                    tracing::debug!(
                        "Copied stylesheet from {} to {}",
                        source_path.display(),
                        dest_path.display()
                    );
                }
            }
        }
    }

    /// Write the syntect CSS file next to the HTML output when `linkcss` is set
    /// and class-based syntax highlighting is active.
    ///
    /// Analogous to how asciidoctor writes `asciidoctor-coderay.css` /
    /// `asciidoctor-pygments.css` alongside the output.
    #[cfg(feature = "highlighting")]
    fn handle_copy_syntax_css(&self, doc: &Document, html_path: &Path) {
        let linkcss = doc.attributes.get("linkcss").is_some();
        if !linkcss {
            return;
        }

        let processor_attrs = &doc.attributes;
        let source_highlighter_set = processor_attrs
            .get("source-highlighter")
            .is_some_and(|v| !matches!(v, AttributeValue::Bool(false)));
        if !source_highlighter_set {
            return;
        }

        // Build a temporary processor to resolve settings from doc attributes
        let (theme_name, mode) = {
            let tmp = Processor {
                document_attributes: doc.attributes.clone(),
                ..self.clone()
            };
            crate::resolve_highlight_settings(&tmp)
        };

        if mode != syntax::HighlightMode::Class {
            return;
        }

        let Ok(css) = syntax::highlight_css(&theme_name) else {
            return;
        };

        let output_dir = html_path.parent().unwrap_or_else(|| Path::new("."));

        let stylesdir = doc
            .attributes
            .get("stylesdir")
            .map_or_else(|| STYLESDIR_DEFAULT.to_string(), ToString::to_string);

        let dest_dir = if stylesdir.is_empty() || stylesdir == STYLESDIR_DEFAULT {
            output_dir.to_path_buf()
        } else if Path::new(&stylesdir).is_absolute() {
            PathBuf::from(&stylesdir)
        } else {
            output_dir.join(&stylesdir)
        };

        let dest_path = dest_dir.join(SYNTECT_STYLESHEET);

        if let Err(e) = std::fs::create_dir_all(&dest_dir) {
            tracing::warn!(
                path = %dest_dir.display(),
                "could not create stylesdir for syntax CSS: {e}"
            );
            return;
        }

        if let Err(e) = std::fs::write(&dest_path, css) {
            tracing::warn!(
                path = %dest_path.display(),
                "could not write syntax highlighting stylesheet: {e}"
            );
        } else {
            tracing::debug!(
                "Wrote syntax highlighting stylesheet to {}",
                dest_path.display()
            );
        }
    }

    #[cfg(not(feature = "highlighting"))]
    fn handle_copy_syntax_css(&self, _doc: &Document, _html_path: &Path) {}
}

/// Build a class string from a base class and optional roles
pub(crate) fn build_class(base: &str, roles: &[String]) -> String {
    if roles.is_empty() {
        base.to_string()
    } else {
        format!("{base} {}", roles.join(" "))
    }
}

/// Write attribution div for quote/verse blocks if author or citation present
pub(crate) fn write_attribution<W: std::io::Write>(
    writer: &mut W,
    metadata: &acdc_parser::BlockMetadata,
) -> Result<(), std::io::Error> {
    let author = metadata.attributes.get_string("attribution");
    let citation = metadata.attributes.get_string("citation");

    if author.is_some() || citation.is_some() {
        writeln!(writer, "<div class=\"attribution\">")?;
        match (author, &citation) {
            (Some(author), Some(citation)) => {
                writeln!(writer, "&#8212; {author}<br>\n<cite>{citation}</cite>")?;
            }
            (Some(author), None) => writeln!(writer, "&#8212; {author}")?,
            (None, Some(citation)) => writeln!(writer, "<cite>{citation}</cite>")?,
            (None, None) => {}
        }
        writeln!(writer, "</div>")?;
    }
    Ok(())
}

/// Write semantic attribution as `<footer>` inside a `<blockquote>` for html5s mode.
/// Format: `<footer>&#8212; <cite>Author, Citation</cite></footer>`
pub(crate) fn write_semantic_attribution<W: std::io::Write>(
    writer: &mut W,
    metadata: &acdc_parser::BlockMetadata,
) -> Result<(), std::io::Error> {
    let author = metadata.attributes.get_string("attribution");
    let citation = metadata.attributes.get_string("citation");

    if author.is_some() || citation.is_some() {
        match (author, &citation) {
            (Some(author), Some(citation)) => {
                writeln!(
                    writer,
                    "<footer>&#8212; <cite>{author}, {citation}</cite></footer>"
                )?;
            }
            (Some(author), None) => {
                writeln!(writer, "<footer>&#8212; <cite>{author}</cite></footer>")?;
            }
            (None, Some(citation)) => {
                writeln!(writer, "<footer>&#8212; <cite>{citation}</cite></footer>")?;
            }
            (None, None) => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_converters_core::Converter;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_convert_to_string_embedded_no_document_frame() -> TestResult {
        let content = r"= My Title

This is a paragraph.

== Section One

* Item 1
* Item 2
";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let render_options = RenderOptions {
            embedded: true,
            ..RenderOptions::default()
        };
        let html = processor.convert_to_string(&doc, &render_options)?;

        // Should NOT contain document frame elements
        assert!(!html.contains("<!DOCTYPE"), "should not contain DOCTYPE");
        assert!(!html.contains("<html"), "should not contain <html>");
        assert!(!html.contains("<head"), "should not contain <head>");
        assert!(!html.contains("<body"), "should not contain <body>");
        assert!(!html.contains("</html>"), "should not contain </html>");
        assert!(!html.contains("</body>"), "should not contain </body>");
        assert!(
            !html.contains("<div id=\"footer\">"),
            "should not contain footer"
        );

        // Should contain the title as h1
        assert!(
            !html.contains("<h1>My Title</h1>"),
            "should not contain title as h1"
        );

        // Should contain body content with wrapper divs
        assert!(
            html.contains("<div class=\"paragraph\">"),
            "should contain paragraph wrapper"
        );
        assert!(
            html.contains("<div class=\"ulist\">"),
            "should contain list wrapper"
        );
        assert!(
            html.contains("<div class=\"sect1\">"),
            "should contain section wrapper"
        );

        Ok(())
    }

    #[test]
    fn test_section_numbering_disabled_by_default() -> TestResult {
        let content = r"= Title

== Section One

== Section Two
";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let html = processor.convert_to_string(&doc, &RenderOptions::default())?;

        assert!(
            html.contains(">Section One</h2>"),
            "section title should appear without number"
        );
        Ok(())
    }

    #[test]
    fn test_section_numbering_enabled() -> TestResult {
        let content = r"= Title
:sectnums:

== Section One

== Section Two

== Section Three
";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let html = processor.convert_to_string(&doc, &RenderOptions::default())?;

        assert!(
            html.contains(">1. Section One</h2>"),
            "first section should be numbered 1."
        );
        assert!(
            html.contains(">2. Section Two</h2>"),
            "second section should be numbered 2."
        );
        assert!(
            html.contains(">3. Section Three</h2>"),
            "third section should be numbered 3."
        );
        Ok(())
    }

    #[test]
    fn test_section_numbering_nested() -> TestResult {
        let content = r"= Title
:sectnums:

== Chapter One

=== Section 1.1

=== Section 1.2

== Chapter Two

=== Section 2.1
";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let html = processor.convert_to_string(&doc, &RenderOptions::default())?;

        assert!(
            html.contains(">1. Chapter One</h2>"),
            "chapter 1 should be numbered"
        );
        assert!(
            html.contains(">1.1. Section 1.1</h3>"),
            "section 1.1 should have hierarchical numbering"
        );
        assert!(
            html.contains(">1.2. Section 1.2</h3>"),
            "section 1.2 should have hierarchical numbering"
        );
        assert!(
            html.contains(">2. Chapter Two</h2>"),
            "chapter 2 should be numbered"
        );
        assert!(
            html.contains(">2.1. Section 2.1</h3>"),
            "section 2.1 should reset subsection counter"
        );
        Ok(())
    }

    #[test]
    fn test_section_numbering_respects_sectnumlevels() -> TestResult {
        let content = r"= Title
:sectnums:
:sectnumlevels: 2

== Chapter One

=== Section 1.1

==== Subsection 1.1.1
";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let html = processor.convert_to_string(&doc, &RenderOptions::default())?;

        assert!(
            html.contains(">1. Chapter One</h2>"),
            "level 1 should be numbered"
        );
        assert!(
            html.contains(">1.1. Section 1.1</h3>"),
            "level 2 should be numbered"
        );
        // Level 3 should NOT be numbered when sectnumlevels=2
        assert!(
            html.contains(">Subsection 1.1.1</h4>"),
            "level 3 should not be numbered when sectnumlevels=2"
        );
        Ok(())
    }

    #[test]
    fn test_unnumbered_section_styles() -> TestResult {
        let content = r"= Title
:sectnums:

== Introduction

[bibliography]
== Bibliography

== Conclusion
";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let html = processor.convert_to_string(&doc, &RenderOptions::default())?;

        assert!(
            html.contains(">1. Introduction</h2>"),
            "introduction should be numbered"
        );
        // Bibliography should NOT be numbered (special section)
        assert!(
            html.contains(">Bibliography</h2>"),
            "bibliography should not be numbered"
        );
        // Counter should continue after unnumbered section
        assert!(
            html.contains(">2. Conclusion</h2>"),
            "conclusion should continue numbering after unnumbered section"
        );
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // TOC numbering integration tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_toc_section_numbers_disabled() -> TestResult {
        let content = r"= Title
:toc:

== Section One

== Section Two
";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let html = processor.convert_to_string(&doc, &RenderOptions::default())?;

        // TOC should exist
        assert!(html.contains("<div id=\"toc\""), "should have TOC");
        // Without :sectnums:, TOC entries should NOT have numbers
        assert!(
            !html.contains("<a href=\"#_section_one\">1."),
            "TOC entry should not have number without sectnums"
        );
        Ok(())
    }

    #[test]
    fn test_toc_section_numbers_enabled() -> TestResult {
        let content = r"= Title
:toc:
:sectnums:

== Section One

== Section Two

== Section Three
";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let html = processor.convert_to_string(&doc, &RenderOptions::default())?;

        // TOC should have numbered entries
        assert!(
            html.contains("<a href=\"#_section_one\">1. Section One</a>"),
            "TOC entry 1 should be numbered"
        );
        assert!(
            html.contains("<a href=\"#_section_two\">2. Section Two</a>"),
            "TOC entry 2 should be numbered"
        );
        assert!(
            html.contains("<a href=\"#_section_three\">3. Section Three</a>"),
            "TOC entry 3 should be numbered"
        );
        Ok(())
    }

    #[test]
    fn test_toc_unnumbered_sections_skipped() -> TestResult {
        let content = r"= Title
:toc:
:sectnums:

== Introduction

[glossary]
== Glossary

== Conclusion
";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let html = processor.convert_to_string(&doc, &RenderOptions::default())?;

        // TOC should have Introduction numbered as 1
        assert!(
            html.contains("<a href=\"#_introduction\">1. Introduction</a>"),
            "introduction should be numbered in TOC"
        );
        // Glossary should NOT be numbered in TOC
        assert!(
            html.contains("<a href=\"#_glossary\">Glossary</a>"),
            "glossary should not be numbered in TOC"
        );
        // Conclusion should continue numbering as 2
        assert!(
            html.contains("<a href=\"#_conclusion\">2. Conclusion</a>"),
            "conclusion should be numbered 2 in TOC (continuing after unnumbered)"
        );
        Ok(())
    }

    #[test]
    fn test_toc_nested_numbering() -> TestResult {
        let content = r"= Title
:toc:
:sectnums:

== Chapter One

=== Section 1.1

== Chapter Two
";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let html = processor.convert_to_string(&doc, &RenderOptions::default())?;

        assert!(
            html.contains("<a href=\"#_chapter_one\">1. Chapter One</a>"),
            "chapter 1 should be numbered in TOC"
        );
        assert!(
            html.contains("<a href=\"#_section_1_1\">1.1. Section 1.1</a>"),
            "section 1.1 should have hierarchical numbering in TOC"
        );
        assert!(
            html.contains("<a href=\"#_chapter_two\">2. Chapter Two</a>"),
            "chapter 2 should be numbered in TOC"
        );
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Dark mode tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_dark_mode_includes_dark_css_and_meta() -> TestResult {
        let content = "= Title\n:dark-mode:\n\nHello world.\n";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let html = processor.convert_to_string(&doc, &RenderOptions::default())?;

        assert!(
            html.contains(r#"<meta name="color-scheme" content="dark">"#),
            "should include color-scheme meta tag"
        );
        assert!(
            html.contains("color-scheme:dark"),
            "should include dark mode CSS"
        );
        assert!(
            html.contains(r#"class="article dark"#),
            "body should have dark class"
        );
        Ok(())
    }

    #[test]
    fn test_light_mode_no_dark_css() -> TestResult {
        let content = "= Title\n:light-mode:\n\nHello world.\n";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let html = processor.convert_to_string(&doc, &RenderOptions::default())?;

        assert!(
            !html.contains(r#"<meta name="color-scheme" content="dark">"#),
            "should not include dark color-scheme meta"
        );
        assert!(
            !html.contains("color-scheme:dark"),
            "should not include dark mode CSS"
        );
        assert!(
            !html.contains("class=\"article dark"),
            "body should not have dark class"
        );
        Ok(())
    }

    #[test]
    fn test_default_no_dark_css() -> TestResult {
        let content = "= Title\n\nHello world.\n";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let html = processor.convert_to_string(&doc, &RenderOptions::default())?;

        assert!(
            !html.contains("color-scheme:dark"),
            "default should not include dark mode CSS"
        );
        assert!(
            !html.contains("class=\"article dark"),
            "default body should not have dark class"
        );
        Ok(())
    }

    #[test]
    fn test_dark_mode_unset_no_dark_css() -> TestResult {
        let content = "= Title\n:!dark-mode:\n\nHello world.\n";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let html = processor.convert_to_string(&doc, &RenderOptions::default())?;

        assert!(
            !html.contains("color-scheme:dark"),
            "unset dark-mode should not include dark CSS"
        );
        Ok(())
    }

    #[test]
    fn test_book_doctype_body_class() -> TestResult {
        let content = "= Book Title\n:doctype: book\n\nSome content.\n";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let html = processor.convert_to_string(&doc, &RenderOptions::default())?;

        assert!(
            html.contains("<body class=\"book\">"),
            "body class should be 'book' when :doctype: book is set"
        );
        Ok(())
    }

    #[test]
    fn test_book_with_parts() -> TestResult {
        let content = r"= Book Title
:doctype: book

= Part One

== Chapter One

Content.
";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let html = processor.convert_to_string(&doc, &RenderOptions::default())?;

        assert!(
            html.contains("<h1 id=\"_part_one\" class=\"sect0\">Part One</h1>"),
            "level 0 section should render as standalone h1 with class=\"sect0\""
        );
        assert!(
            !html.contains("<div class=\"sect0\">"),
            "level 0 section should NOT have a wrapper div"
        );
        assert!(
            html.contains("<div class=\"sect1\">"),
            "chapter should render as sect1"
        );
        Ok(())
    }

    #[test]
    fn test_book_toc_includes_parts() -> TestResult {
        let content = r"= Book Title
:doctype: book
:toc:

= Part One

== Chapter One

Content.

= Part Two

== Chapter Two

Content.
";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let html = processor.convert_to_string(&doc, &RenderOptions::default())?;

        assert!(
            html.contains("<ul class=\"sectlevel0\">"),
            "TOC should include level 0 entries"
        );
        assert!(
            html.contains("<a href=\"#_part_one\">Part One</a>"),
            "TOC should contain Part One link"
        );
        assert!(
            html.contains("<a href=\"#_part_two\">Part Two</a>"),
            "TOC should contain Part Two link"
        );
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Appendix rendering tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_appendix_level0_demoted_to_sect1() -> TestResult {
        let content = r"= Book Title
:doctype: book

= Part One

== Chapter One

Content.

[appendix]
= First Appendix

Appendix content.

[appendix]
= Second Appendix

More appendix content.
";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let html = processor.convert_to_string(&doc, &RenderOptions::default())?;

        // Appendix sections should be demoted to sect1 with h2
        assert!(
            html.contains(
                "<div class=\"sect1\">\n<h2 id=\"_first_appendix\">Appendix A: First Appendix</h2>"
            ),
            "first appendix should render as sect1 with 'Appendix A: ' prefix"
        );
        assert!(
            html.contains("<div class=\"sect1\">\n<h2 id=\"_second_appendix\">Appendix B: Second Appendix</h2>"),
            "second appendix should render as sect1 with 'Appendix B: ' prefix"
        );

        // Appendix sections should have sectionbody wrapper
        assert!(
            html.contains(
                "<div class=\"sectionbody\">\n<div class=\"paragraph\">\n<p>Appendix content.</p>"
            ),
            "appendix should have sectionbody wrapper"
        );

        // Part should still render as h1 sect0
        assert!(
            html.contains("<h1 id=\"_part_one\" class=\"sect0\">Part One</h1>"),
            "part should remain as h1 sect0"
        );

        Ok(())
    }

    #[test]
    fn test_appendix_toc_entries() -> TestResult {
        let content = r"= Book Title
:doctype: book
:toc:

= Part One

== Chapter One

Content.

[appendix]
= First Appendix

Content.

[appendix]
= Second Appendix

Content.
";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let html = processor.convert_to_string(&doc, &RenderOptions::default())?;

        // TOC should have appendix entries with letter prefix
        assert!(
            html.contains("<a href=\"#_first_appendix\">Appendix A: First Appendix</a>"),
            "TOC should contain 'Appendix A: First Appendix'"
        );
        assert!(
            html.contains("<a href=\"#_second_appendix\">Appendix B: Second Appendix</a>"),
            "TOC should contain 'Appendix B: Second Appendix'"
        );

        Ok(())
    }

    #[test]
    fn test_appendix_custom_caption() -> TestResult {
        let content = r"= Book Title
:doctype: book
:appendix-caption: Annexe

[appendix]
= First Appendix

Content.
";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let html = processor.convert_to_string(&doc, &RenderOptions::default())?;

        assert!(
            html.contains(">Annexe A: First Appendix</h2>"),
            "should use custom appendix caption"
        );

        Ok(())
    }

    #[test]
    fn test_appendix_disabled_caption() -> TestResult {
        let content = r"= Book Title
:doctype: book
:!appendix-caption:

[appendix]
= First Appendix

Content.
";
        let parser_options = acdc_parser::Options::default();
        let doc = acdc_parser::parse(content, &parser_options)?;

        let processor = Processor::new(
            acdc_converters_core::Options::default(),
            doc.attributes.clone(),
        );
        let html = processor.convert_to_string(&doc, &RenderOptions::default())?;

        // Should still be demoted to sect1/h2, just no prefix
        assert!(
            html.contains("<div class=\"sect1\">\n<h2 id=\"_first_appendix\">First Appendix</h2>"),
            "appendix with disabled caption should have no prefix but still be demoted"
        );

        Ok(())
    }
}
