//! Manpage converter for `AsciiDoc` documents.
//!
//! This converter outputs native roff/troff format suitable for the `man` command.
//! It targets modern GNU groff and produces semantically equivalent output to
//! Asciidoctor's manpage backend.
//!
//! # Example
//!
//! ```ignore
//! use acdc_converters_manpage::Processor;
//! use acdc_converters_core::{Converter, Options};
//!
//! let options = Options::default();
//! let processor = Processor::new(options, Default::default());
//! processor.convert(&document, Some(Path::new("cmd.adoc")))?;
//! // Outputs: cmd.1 (or other extension based on volume number)
//! ```
//!
//! # Output Format
//!
//! The converter generates roff output with:
//! - `.TH` header with program name, volume, date, source, and manual
//! - `.SH` and `.SS` macros for section headings
//! - `.PP`, `.IP`, `.TP` for paragraphs and list items
//! - `.EX`/`.EE` for code examples
//! - `.TS`/`.TE` for tables (tbl preprocessor format)
//! - `\fB`, `\fI`, `\fP` for inline formatting

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use acdc_converters_core::{Backend, Converter, Options, visitor::Visitor};
use acdc_parser::{AttributeValue, Document, DocumentAttributes};

mod admonition;
mod delimited;
mod document;
mod error;
mod escape;
mod inlines;
mod list;
mod manpage_visitor;
mod paragraph;
mod section;
mod table;

pub use error::Error;
pub use escape::{EscapeMode, manify};
pub use manpage_visitor::ManpageVisitor;

/// Manpage converter processor.
#[derive(Clone, Debug)]
pub struct Processor {
    options: Options,
    document_attributes: DocumentAttributes,
}

impl Processor {
    /// Determine the output file extension based on the volume number.
    fn output_extension(doc: &Document) -> String {
        // Read manvolnum from document attributes (set by parser)
        doc.attributes
            .get("manvolnum")
            .and_then(|v| match v {
                acdc_parser::AttributeValue::String(s) => Some(s.clone()),
                acdc_parser::AttributeValue::Bool(_) | acdc_parser::AttributeValue::None | _ => {
                    None
                }
            })
            .unwrap_or_else(|| String::from("1"))
    }
}

impl Converter for Processor {
    type Error = Error;

    fn document_attributes_defaults() -> DocumentAttributes {
        let mut attrs = DocumentAttributes::default();
        // man-linkstyle controls how links are rendered in the manpage
        // Format: "color style <text>" - blue R <> means blue, regular, angle brackets
        attrs.insert(
            "man-linkstyle".into(),
            AttributeValue::String("blue R <>".into()),
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
        }
    }

    fn options(&self) -> &Options {
        &self.options
    }

    fn document_attributes(&self) -> &DocumentAttributes {
        &self.document_attributes
    }

    fn derive_output_path(&self, input: &Path, doc: &Document) -> Result<Option<PathBuf>, Error> {
        let extension = Self::output_extension(doc);
        let manpage_path = input.with_extension(&extension);
        // Avoid overwriting the input file
        if manpage_path == input {
            return Err(Error::OutputPathSameAsInput(input.to_path_buf()));
        }
        Ok(Some(manpage_path))
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
        let mut visitor = ManpageVisitor::new(writer, processor);
        visitor.visit_document(doc)?;
        Ok(())
    }

    fn backend(&self) -> Backend {
        Backend::Manpage
    }
}
