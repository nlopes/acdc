use std::{
    io::Write,
    path::{Path, PathBuf},
};

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::SubsFlags;
use acdc_converters_core::{Converter, Diagnostics, Options, WarningSource};
use acdc_parser::{Document, DocumentAttributes};
#[cfg(feature = "pre-spec-subs")]
use std::{cell::Cell, rc::Rc};

use crate::{Error, PdfOptions, Processor};

impl<'a> Converter<'a> for Processor<'a> {
    type Error = Error;

    fn new(options: Options, document_attributes: DocumentAttributes<'a>) -> Self {
        Self {
            options,
            document_attributes,
            pdf_options: PdfOptions::default(),
            #[cfg(feature = "pre-spec-subs")]
            current_subs: Rc::new(Cell::new(SubsFlags::all())),
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
        source_file: Option<&Path>,
        _output_path: Option<&Path>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<(), Self::Error> {
        let rendered = self.render_document(doc, source_file, diagnostics)?;
        if self.options().timings() {
            rendered
                .timings
                .write_report(rendered.resolved_document_image_count);
        }
        writer.write_all(&rendered.pdf)?;
        Ok(())
    }

    fn name(&self) -> &'static str {
        "pdf"
    }

    fn warning_source(&self) -> WarningSource {
        WarningSource::new("pdf")
    }
}
