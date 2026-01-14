use std::{
    cell::{Cell, RefCell},
    io::Write,
    path::{Path, PathBuf},
    rc::Rc,
};

use acdc_converters_core::{Backend, Converter, Options, visitor::Visitor};
use acdc_parser::{AttributeValue, Block, Document, DocumentAttributes, IndexTermKind, TocEntry};

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
        let processor = Processor {
            toc_entries: doc.toc_entries.clone(),
            document_attributes: doc.attributes.clone(),
            has_valid_index_section: Self::last_section_is_index(&doc.blocks),
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
pub struct RenderOptions {
    pub last_updated: Option<chrono::DateTime<chrono::Utc>>,
    pub inlines_basic: bool,
    pub inlines_verbatim: bool,
    /// When true, output embeddable document (no DOCTYPE, html, head, body wrapper).
    /// Follows Asciidoctor's embedded mode behavior - excludes header/footer frame
    /// but keeps body content structure including wrapper divs.
    pub embedded: bool,
}

pub(crate) const COPYCSS_DEFAULT: &str = "";
pub(crate) const STYLESDIR_DEFAULT: &str = ".";
pub(crate) const STYLESHEET_DEFAULT: &str = "";
pub(crate) const STYLESHEET_FILENAME_DEFAULT: &str = "asciidoctor.css";
pub(crate) const WEBFONTS_DEFAULT: &str = "";

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
        let mut document_attributes = document_attributes;
        for (name, value) in Self::document_attributes_defaults().iter() {
            document_attributes.insert(name.clone(), value.clone());
        }

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
        }
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
            ..RenderOptions::default()
        };
        self.convert_to_writer(doc, writer, &render_options)
    }

    fn after_write(&self, doc: &Document, output_path: &Path) {
        Self::handle_copycss(doc, output_path);
    }

    fn backend(&self) -> Backend {
        Backend::Html
    }
}

impl Processor {
    /// Handle copying CSS if linkcss and copycss are set.
    fn handle_copycss(doc: &acdc_parser::Document, html_path: &std::path::Path) {
        let linkcss = doc.attributes.get("linkcss").is_some();
        if !linkcss {
            return;
        }

        let should_copy = doc.attributes.contains_key("copycss");
        tracing::debug!("linkcss={linkcss}, copycss exists={should_copy}");

        if !should_copy {
            return;
        }

        let stylesheet = doc
            .attributes
            .get("stylesheet")
            .and_then(|v| {
                let s = v.to_string();
                if s.is_empty() { None } else { Some(s) }
            })
            .unwrap_or_else(|| STYLESHEET_FILENAME_DEFAULT.into());

        let stylesdir = doc
            .attributes
            .get("stylesdir")
            .map_or_else(|| STYLESDIR_DEFAULT.into(), ToString::to_string);

        let source_path = if stylesdir.is_empty() || stylesdir == STYLESDIR_DEFAULT {
            std::path::Path::new(&stylesheet).to_path_buf()
        } else {
            std::path::Path::new(&stylesdir).join(&stylesheet)
        };

        let output_dir = html_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let dest_path = output_dir.join(&stylesheet);

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

mod admonition;
mod audio;
mod constants;
mod delimited;
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
mod table;
mod toc;
mod video;

pub use error::Error;
pub use html_visitor::HtmlVisitor;

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
}
