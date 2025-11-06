use std::{
    cell::Cell,
    fs::File,
    io::{self, BufWriter, Write},
    rc::Rc,
    time::Instant,
};

use acdc_converters_common::{Options, PrettyDuration, Processable, visitor::Visitor};
use acdc_parser::{Document, DocumentAttributes, TocEntry};

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
            ..self.clone()
        };
        let mut visitor = HtmlVisitor::new(writer, processor, options.clone());
        visitor.visit_document(doc)?;
        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
pub struct RenderOptions {
    pub last_updated: Option<chrono::DateTime<chrono::Utc>>,
    pub inlines_basic: bool,
    pub inlines_verbatim: bool,
}

impl Processable for Processor {
    type Options = Options;
    type Error = Error;

    fn new(options: Options, document_attributes: DocumentAttributes) -> Self {
        Self {
            options,
            document_attributes,
            toc_entries: vec![],
            example_counter: Rc::new(Cell::new(0)),
            table_counter: Rc::new(Cell::new(0)),
        }
    }

    fn convert(
        &self,
        doc: &acdc_parser::Document,
        file: Option<&std::path::Path>,
    ) -> Result<(), Self::Error> {
        if let Some(file_path) = file {
            // File-based conversion - write to .html file
            let html_path = file_path.with_extension("html");
            if html_path == file_path {
                return Err(Error::OutputPathSameAsInput(file_path.to_path_buf()));
            }

            let render_options = RenderOptions {
                last_updated: Some(
                    std::fs::metadata(file_path)?
                        .modified()
                        .map(chrono::DateTime::from)?,
                ),
                ..RenderOptions::default()
            };

            if self.options.timings {
                println!("Input file: {}", file_path.display());
            }
            tracing::debug!(source = ?file_path, destination = ?html_path, "converting document");

            let now = Instant::now();
            let file_handle = File::create(&html_path)?;
            let writer = BufWriter::new(file_handle);
            self.convert_to_writer(doc, writer, &render_options)?;
            let elapsed = now.elapsed();
            tracing::debug!(time = elapsed.pretty_print_precise(3), source = ?file_path, destination = ?html_path, "time to convert document");

            if self.options.timings {
                println!("  Time to convert document: {}", elapsed.pretty_print());
            }
            println!("Generated HTML file: {}", html_path.display());

            Ok(())
        } else {
            // Stdin-based conversion - write to stdout
            let render_options = RenderOptions::default();
            let stdout = io::stdout();
            let writer = BufWriter::new(stdout.lock());
            self.convert_to_writer(doc, writer, &render_options)?;
            Ok(())
        }
    }
}

mod admonition;
mod audio;
mod delimited;
mod document;
mod error;
mod html_visitor;
mod image;
mod inlines;
mod list;
mod paragraph;
mod section;
mod table;
mod toc;
mod video;

pub(crate) use error::Error;
pub use html_visitor::HtmlVisitor;
