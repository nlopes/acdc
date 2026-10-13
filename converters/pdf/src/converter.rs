use std::{
    cell::Cell,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    rc::Rc,
    time::Instant,
};

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::SubsFlags;
use acdc_converters_core::{
    ConversionResult, Converter, Diagnostics, Options, PrettyDuration, WarningSource,
};
use acdc_parser::Document;

use crate::{Error, PDF_BACKEND, PdfOptions, Processor};

impl<'a> Converter<'a> for Processor<'a> {
    type Error = Error;

    fn new(
        options: Options,
        parser_options: acdc_parser::OptionsBuilder<'a>,
    ) -> Result<Self, Self::Error> {
        let mut parser_options = parser_options;
        parser_options = PDF_BACKEND.apply(parser_options, options.doctype(), options.embedded());
        let parser_options = parser_options.build()?;
        Ok(Self {
            options,
            parser_options,
            references: Rc::new(std::collections::HashMap::new()),
            xref_guard: acdc_converters_core::xref::XrefGuard::default(),
            example_counter: Rc::new(Cell::new(0)),
            figure_counter: Rc::new(Cell::new(0)),
            listing_counter: Rc::new(Cell::new(0)),
            table_counter: Rc::new(Cell::new(0)),
            pdf_options: PdfOptions::default(),
            #[cfg(feature = "pre-spec-subs")]
            current_subs: Rc::new(Cell::new(SubsFlags::all())),
        })
    }

    fn options(&self) -> &Options {
        &self.options
    }

    fn parser_options(&self) -> &acdc_parser::Options<'a> {
        &self.parser_options
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
        output_path: Option<&Path>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<(), Self::Error> {
        self.validate_theme_config_paths(source_file, output_path)?;
        let rendered = self.render_document(doc, source_file, diagnostics)?;
        if self.options().timings() {
            rendered
                .timings
                .write_report(rendered.resolved_document_image_count);
        }
        writer.write_all(&rendered.pdf)?;
        Ok(())
    }

    fn convert_to_file(
        &self,
        doc: &Document<'_>,
        source_file: Option<&Path>,
        output_path: &Path,
    ) -> Result<ConversionResult, Self::Error> {
        // The default method opens the PDF before write_to can reject a collision.
        self.validate_theme_config_paths(source_file, Some(output_path))?;
        let start = self.options().timings().then(Instant::now);
        if let Some(input) = source_file.filter(|_| self.options().timings()) {
            println!("Input file: {}", input.display());
        }
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::File::create(output_path)?;
        let source = self.warning_source();
        let mut warnings = Vec::new();
        self.write_to(
            doc,
            BufWriter::new(file),
            source_file,
            Some(output_path),
            &mut Diagnostics::new(&source, &mut warnings),
        )?;
        if let Some(start) = start {
            let elapsed = start.elapsed();
            println!("  Time to convert document: {}", elapsed.pretty_print());
        }
        println!("Generated {} file: {}", self.name(), output_path.display());
        Ok(ConversionResult::file(output_path.to_path_buf(), warnings))
    }

    fn name(&self) -> &'static str {
        "pdf"
    }

    fn warning_source(&self) -> WarningSource {
        WarningSource::new("pdf")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_applies_pdf_backend_profile() -> Result<(), Box<dyn std::error::Error>> {
        let processor = Processor::new(Options::default(), acdc_parser::Options::builder())?;

        assert_eq!(
            processor
                .document_attributes()
                .get("backend")
                .and_then(|value| value.text()),
            Some("pdf")
        );
        assert_eq!(
            processor
                .document_attributes()
                .get("basebackend")
                .and_then(|value| value.text()),
            Some("html")
        );
        assert!(processor.document_attributes().contains_key("backend-pdf"));
        assert!(
            !processor
                .document_attributes()
                .contains_key("part-signifier")
        );
        Ok(())
    }
}
