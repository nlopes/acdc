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
    borrow::Cow,
    io::Write,
    path::{Path, PathBuf},
};

use acdc_converters_core::{Converter, Diagnostics, Options, visitor::Visitor};

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
pub struct Processor<'a> {
    options: Options,
    document_attributes: DocumentAttributes<'a>,
}

impl<'a> Processor<'a> {
    /// Convert a document to manpage output, writing to the provided writer.
    ///
    /// # Errors
    ///
    /// Returns an error if conversion or writing fails.
    pub fn write_document<W: Write>(
        &self,
        doc: &Document<'a>,
        writer: W,
        source_file: Option<&Path>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<(), Error> {
        let mut attrs: DocumentAttributes<'a> = doc.attributes.clone();

        if !attrs.contains_key("revdate")
            && let Some(date_str) = source_file.and_then(file_modified_date)
        {
            attrs.insert(
                "revdate".into(),
                AttributeValue::String(Cow::Owned(date_str)),
            );
        }

        let processor = Processor {
            options: self.options.clone(),
            document_attributes: attrs,
        };
        let mut visitor = ManpageVisitor::new(writer, processor, diagnostics.reborrow());
        visitor.visit_document(doc)
    }

    /// Determine the output file extension based on the volume number.
    fn output_extension(doc: &Document) -> String {
        // Read manvolnum from document attributes (set by parser)
        doc.attributes
            .get("manvolnum")
            .and_then(|v| match v {
                acdc_parser::AttributeValue::String(s) => Some(s.clone().into_owned()),
                acdc_parser::AttributeValue::Bool(_) | acdc_parser::AttributeValue::None | _ => {
                    None
                }
            })
            .unwrap_or_else(|| String::from("1"))
    }
}

impl<'a> Converter<'a> for Processor<'a> {
    type Error = Error;

    fn document_attributes_defaults() -> DocumentAttributes<'static> {
        let mut attrs: DocumentAttributes<'static> = DocumentAttributes::default();
        // man-linkstyle controls how links are rendered in the manpage
        // Format: "color style <text>" - blue R <> means blue, regular, angle brackets
        attrs.insert("man-linkstyle".into(), "blue R <>".into());
        attrs
    }

    fn new(options: Options, document_attributes: DocumentAttributes<'a>) -> Self {
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

    fn document_attributes(&self) -> &DocumentAttributes<'a> {
        &self.document_attributes
    }

    fn derive_output_path(
        &self,
        input: &Path,
        doc: &Document<'a>,
    ) -> Result<Option<PathBuf>, Error> {
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
        doc: &Document<'a>,
        writer: W,
        source_file: Option<&Path>,
        _output_path: Option<&Path>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<(), Self::Error> {
        self.write_document(doc, writer, source_file, diagnostics)
    }

    fn name(&self) -> &'static str {
        "manpage"
    }
}

/// Get a file's modification date as a `YYYY-MM-DD` string.
fn file_modified_date(path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let datetime: chrono::DateTime<chrono::Local> = modified.into();
    Some(datetime.format("%Y-%m-%d").to_string())
}
