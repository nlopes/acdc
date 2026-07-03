use std::{
    io::Write,
    path::{Path, PathBuf},
};

use acdc_converters_core::{Converter, Diagnostics, Options, WarningSource};
use acdc_parser::{Document, DocumentAttributes};

use crate::{Error, Processor};

impl<'a> Converter<'a> for Processor<'a> {
    type Error = Error;

    fn new(options: Options, document_attributes: DocumentAttributes<'a>) -> Self {
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
        _doc: &Document<'_>,
    ) -> Result<Option<PathBuf>, Error> {
        let pdf_path = input.with_extension("pdf");
        if pdf_path == input {
            return Err(Error::OutputPathSameAsInput(input.to_path_buf()));
        }
        Ok(Some(pdf_path))
    }

    fn write_to<W: Write>(
        &self,
        doc: &Document<'_>,
        mut writer: W,
        _source_file: Option<&Path>,
        _output_path: Option<&Path>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<(), Self::Error> {
        let source = self.convert_to_typst_source(doc, diagnostics)?;
        let pdf = Self::compile_pdf(&source)?;
        writer.write_all(&pdf)?;
        Ok(())
    }

    fn name(&self) -> &'static str {
        "pdf"
    }

    fn warning_source(&self) -> WarningSource {
        WarningSource::new("pdf")
    }
}
