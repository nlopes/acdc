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
//! use acdc_converters_common::{Options, Processable};
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
    fs::File,
    io::{self, BufWriter, Write},
    path::Path,
    time::Instant,
};

use acdc_converters_common::{Options, PrettyDuration, Processable, visitor::Visitor};
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
    /// Convert a document to manpage format, writing to the provided writer.
    ///
    /// # Errors
    ///
    /// Returns an error if document conversion or writing fails.
    pub fn convert_to_writer<W: Write>(&self, doc: &Document, writer: W) -> Result<(), Error> {
        let processor = Processor {
            document_attributes: doc.attributes.clone(),
            ..self.clone()
        };
        let mut visitor = ManpageVisitor::new(writer, processor);
        visitor.visit_document(doc)?;
        Ok(())
    }

    /// Determine the output file extension based on the volume number.
    fn output_extension(doc: &Document) -> String {
        // Read manvolnum from document attributes (set by parser)
        doc.attributes
            .get("manvolnum")
            .and_then(|v| match v {
                acdc_parser::AttributeValue::String(s) => Some(s.clone()),
                acdc_parser::AttributeValue::Bool(_)
                | acdc_parser::AttributeValue::None
                | acdc_parser::AttributeValue::Inlines(_) => None,
            })
            .unwrap_or_else(|| String::from("1"))
    }
}

impl Processable for Processor {
    type Options = Options;
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

    fn convert(&self, doc: &Document, file: Option<&Path>) -> Result<(), Self::Error> {
        if let Some(file_path) = file {
            // File-based conversion - write to .N file (where N is volume number)
            let extension = Self::output_extension(doc);
            let manpage_path = file_path.with_extension(&extension);

            if manpage_path == file_path {
                return Err(Error::OutputPathSameAsInput(file_path.to_path_buf()));
            }

            if self.options.timings {
                println!("Input file: {}", file_path.display());
            }
            tracing::debug!(
                source = ?file_path,
                destination = ?manpage_path,
                "converting document to manpage"
            );

            let now = Instant::now();
            let file_handle = File::create(&manpage_path)?;
            let writer = BufWriter::new(file_handle);
            self.convert_to_writer(doc, writer)?;
            let elapsed = now.elapsed();

            tracing::debug!(
                time = elapsed.pretty_print_precise(3),
                source = ?file_path,
                destination = ?manpage_path,
                "time to convert document"
            );

            if self.options.timings {
                println!("  Time to convert document: {}", elapsed.pretty_print());
            }
            println!("Generated manpage file: {}", manpage_path.display());

            Ok(())
        } else {
            // Stdin-based conversion - write to stdout
            let stdout = io::stdout();
            let writer = BufWriter::new(stdout.lock());
            self.convert_to_writer(doc, writer)?;
            Ok(())
        }
    }

    fn document_attributes(&self) -> DocumentAttributes {
        self.document_attributes.clone()
    }
}
