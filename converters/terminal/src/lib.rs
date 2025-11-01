use std::{
    cell::Cell,
    io::{BufReader, BufWriter, Write},
    rc::Rc,
};

use acdc_converters_common::{Options, Processable, visitor::Visitor};
use acdc_core::Source;
use acdc_parser::{Document, DocumentAttributes, Options as ParserOptions, TocEntry};

pub(crate) const FALLBACK_TERMINAL_WIDTH: usize = 80;

#[derive(Clone, Debug)]
pub struct Processor {
    pub(crate) options: Options,
    pub(crate) document_attributes: DocumentAttributes,
    pub(crate) toc_entries: Vec<TocEntry>,
    /// Shared counter for auto-numbering example blocks.
    /// Uses Rc<Cell<>> so all clones share the same counter.
    pub(crate) example_counter: Rc<Cell<usize>>,
}

impl Processor {
    /// Convert a document to terminal output, writing to the provided writer.
    ///
    /// # Errors
    ///
    /// Returns an error if document conversion or writing fails.
    pub fn convert<W: Write>(&self, doc: &Document, writer: W) -> Result<(), Error> {
        let processor = Processor {
            document_attributes: doc.attributes.clone(),
            toc_entries: doc.toc_entries.clone(),
            options: self.options.clone(),
            example_counter: self.example_counter.clone(),
        };
        let mut visitor = TerminalVisitor::new(writer, processor);
        visitor.visit_document(doc)?;
        Ok(())
    }
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

    fn run(&self) -> Result<(), Error> {
        let options = ParserOptions {
            safe_mode: self.options.safe_mode.clone(),
            timings: self.options.timings,
            document_attributes: self.document_attributes.clone(),
        };
        match &self.options.source {
            Source::Files(files) => {
                for file in files {
                    let doc = acdc_parser::parse_file(file, &options)?;
                    let stdout = std::io::stdout();
                    let writer = BufWriter::new(stdout.lock());
                    self.convert(&doc, writer)?;
                }
            }
            Source::Stdin => {
                let stdin = std::io::stdin();
                let mut reader = BufReader::new(stdin.lock());
                let doc = acdc_parser::parse_from_reader(&mut reader, &options)?;
                let stdout = std::io::stdout();
                let writer = BufWriter::new(stdout.lock());
                self.convert(&doc, writer)?;
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
mod image;
mod inlines;
mod list;
mod paragraph;
mod section;
mod table;
mod terminal_visitor;
mod toc;
mod video;

pub(crate) use error::Error;
pub use terminal_visitor::TerminalVisitor;
