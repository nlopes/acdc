use std::{
    cell::Cell,
    fs::File,
    io::{self, BufReader, BufWriter, Write},
    rc::Rc,
    time::{Duration, Instant},
};

use acdc_converters_common::{Options, PrettyDuration, Processable, visitor::Visitor};
use acdc_core::Source;
use acdc_parser::{Document, DocumentAttributes, TocEntry};

#[derive(Clone, Debug)]
pub struct Processor {
    options: Options,
    document_attributes: DocumentAttributes,
    toc_entries: Vec<TocEntry>,
    /// Shared counter for auto-numbering example blocks.
    /// Uses Rc<Cell<>> so all clones share the same counter.
    example_counter: Rc<Cell<usize>>,
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
    pub fn convert<W: Write>(
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
        }
    }

    #[tracing::instrument(skip(self))]
    fn run(&self) -> Result<(), Self::Error> {
        let mut render_options = RenderOptions {
            ..RenderOptions::default()
        };
        let options = acdc_parser::Options {
            safe_mode: self.options.safe_mode.clone(),
            timings: self.options.timings,
            document_attributes: self.document_attributes.clone(),
        };

        match &self.options.source {
            Source::Files(files) => {
                for file in files {
                    render_options.last_updated = Some(
                        std::fs::metadata(file)?
                            .modified()
                            .map(chrono::DateTime::from)?,
                    );
                    if self.options.timings {
                        println!("Input file: {}", file.to_string_lossy());
                    }
                    let html_path = file.with_extension("html");
                    if html_path == *file {
                        return Err(Error::OutputPathSameAsInput(file.clone()));
                    }
                    tracing::debug!(source = ?file, destination = ?html_path, "processing file");

                    // Read and parse the document
                    let now = Instant::now();
                    let mut total_elapsed = Duration::new(0, 0);
                    let doc = acdc_parser::parse_file(file, &options)?;
                    let elapsed = now.elapsed();
                    tracing::debug!(time = elapsed.pretty_print_precise(3), source = ?file, destination = ?html_path, "time to read and parse source");
                    total_elapsed += elapsed;
                    if self.options.timings {
                        println!(
                            "  Time to read and parse source: {}",
                            elapsed.pretty_print()
                        );
                    }

                    // Convert the document (using visitor pattern)
                    let now = Instant::now();
                    let file_handle = File::create(&html_path)?;
                    let writer = BufWriter::new(file_handle);
                    self.convert(&doc, writer, &render_options)?;
                    let elapsed = now.elapsed();
                    tracing::debug!(time = elapsed.pretty_print_precise(3), source = ?file, destination = ?html_path, "time to convert document");
                    total_elapsed += elapsed;
                    tracing::debug!(time = total_elapsed.pretty_print_precise(3), source = ?file, destination = ?html_path, "total time (read, parse and convert)");
                    if self.options.timings {
                        println!("  Time to convert document: {}", elapsed.pretty_print());
                        println!(
                            "  Total time (read, parse and convert): {}",
                            total_elapsed.pretty_print()
                        );
                    }
                    println!("Generated HTML file: {}", html_path.to_string_lossy());
                }
            }
            Source::Stdin => {
                let stdin = io::stdin();
                let mut reader = BufReader::new(stdin.lock());
                let doc = acdc_parser::parse_from_reader(&mut reader, &options)?;
                let stdout = io::stdout();
                let writer = BufWriter::new(stdout.lock());
                self.convert(&doc, writer, &render_options)?;
            }
        }
        Ok(())
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
