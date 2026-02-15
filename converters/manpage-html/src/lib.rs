use std::{
    io::Write,
    path::{Path, PathBuf},
};

use acdc_converters_core::{Backend, Converter, Options, visitor::Visitor};
use acdc_parser::{Document, DocumentAttributes};

mod admonition;
mod css;
mod delimited;
mod document;
mod error;
mod escape;
mod inlines;
mod list;
mod manpage_html_visitor;
mod paragraph;
mod section;
mod table;

pub use css::{MODERN_CSS, TERMINAL_CSS};
pub use error::Error;
pub use manpage_html_visitor::ManpageHtmlVisitor;

#[derive(Clone, Debug)]
pub struct Processor {
    pub(crate) options: Options,
    pub(crate) document_attributes: DocumentAttributes,
}

impl Processor {
    /// Write manpage-styled HTML for the given document to a writer.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails or the document cannot be converted.
    pub fn convert_to_writer<W: Write>(&self, doc: &Document, writer: W) -> Result<(), Error> {
        let processor = Processor {
            document_attributes: doc.attributes.clone(),
            ..self.clone()
        };
        let mut visitor = ManpageHtmlVisitor::new(writer, processor);
        visitor.visit_document(doc)?;
        Ok(())
    }

    /// Convert the document to a manpage-styled HTML string.
    ///
    /// # Errors
    ///
    /// Returns an error if conversion fails.
    pub fn convert_to_string(&self, doc: &Document) -> Result<String, Error> {
        let mut output = Vec::new();
        self.convert_to_writer(doc, &mut output)?;
        Ok(String::from_utf8(output)?)
    }
}

impl Converter for Processor {
    type Error = Error;

    fn document_attributes_defaults() -> DocumentAttributes {
        DocumentAttributes::default()
    }

    fn new(options: Options, document_attributes: DocumentAttributes) -> Self {
        Self {
            options,
            document_attributes,
        }
    }

    fn options(&self) -> &Options {
        &self.options
    }

    fn document_attributes(&self) -> &DocumentAttributes {
        &self.document_attributes
    }

    fn derive_output_path(&self, input: &Path, _doc: &Document) -> Result<Option<PathBuf>, Error> {
        let output = input.with_extension("html");
        if output == input {
            return Err(Error::OutputPathSameAsInput(input.to_path_buf()));
        }
        Ok(Some(output))
    }

    fn write_to<W: Write>(
        &self,
        doc: &Document,
        writer: W,
        _source_file: Option<&Path>,
    ) -> Result<(), Self::Error> {
        self.convert_to_writer(doc, writer)
    }

    fn backend(&self) -> Backend {
        Backend::ManpageHtml
    }
}
