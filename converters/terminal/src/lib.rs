use std::{
    cell::Cell,
    io::Write,
    path::{Path, PathBuf},
    rc::Rc,
};

use acdc_converters_core::{Backend, Converter, Options, visitor::Visitor};
use acdc_parser::{Document, DocumentAttributes, TocEntry};

pub(crate) use appearance::Appearance;

pub(crate) const FALLBACK_TERMINAL_WIDTH: usize = 80;

#[derive(Clone, Debug)]
pub struct Processor {
    pub(crate) options: Options,
    pub(crate) document_attributes: DocumentAttributes,
    pub(crate) toc_entries: Vec<TocEntry>,
    /// Shared counter for auto-numbering example blocks.
    /// Uses Rc<Cell<>> so all clones share the same counter.
    pub(crate) example_counter: Rc<Cell<usize>>,
    /// Terminal appearance (theme, capabilities, colors)
    pub appearance: Appearance,
}

impl Converter for Processor {
    type Error = Error;

    fn document_attributes_defaults() -> DocumentAttributes {
        // Terminal converter uses environment detection (Appearance::detect())
        // rather than document attributes for its configuration.
        // No terminal-specific attribute defaults needed.
        DocumentAttributes::default()
    }

    fn new(options: Options, document_attributes: DocumentAttributes) -> Self {
        let mut document_attributes = document_attributes;
        for (name, value) in Self::document_attributes_defaults().iter() {
            document_attributes.insert(name.clone(), value.clone());
        }
        let appearance = Appearance::detect();

        Self {
            options,
            document_attributes,
            toc_entries: vec![],
            example_counter: Rc::new(Cell::new(0)),
            appearance,
        }
    }

    fn options(&self) -> &Options {
        &self.options
    }

    fn document_attributes(&self) -> &DocumentAttributes {
        &self.document_attributes
    }

    fn derive_output_path(&self, _input: &Path, _doc: &Document) -> Result<Option<PathBuf>, Error> {
        // Terminal converter always outputs to stdout by default
        Ok(None)
    }

    fn write_to<W: Write>(
        &self,
        doc: &Document,
        writer: W,
        _source_file: Option<&Path>,
    ) -> Result<(), Self::Error> {
        let processor = Processor {
            document_attributes: doc.attributes.clone(),
            toc_entries: doc.toc_entries.clone(),
            options: self.options.clone(),
            example_counter: self.example_counter.clone(),
            appearance: self.appearance.clone(),
        };
        let mut visitor = TerminalVisitor::new(writer, processor);
        visitor.visit_document(doc)?;
        Ok(())
    }

    fn backend(&self) -> Backend {
        Backend::Terminal
    }
}

mod admonition;
mod appearance;
mod audio;
mod delimited;
mod document;
mod error;
mod image;
mod inlines;
mod list;
mod paragraph;
mod section;
mod syntax;
mod table;
mod terminal_visitor;
mod toc;
mod video;

pub use appearance::Capabilities;
pub use error::Error;
pub use terminal_visitor::TerminalVisitor;
