//! Generate expected Typst output files for PDF integration tests.
//!
//! Usage:
//!   `cargo run -p acdc-converters-pdf --example generate_typst_fixtures --all-features`

use acdc_converters_core::{Converter, Options};
use acdc_converters_dev::generate_fixtures::FixtureGenerator;
use acdc_converters_pdf::{PdfOptions, Processor};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    FixtureGenerator::new("pdf", "typ").generate(|_, doc, output| {
        let output_dir = tempfile::tempdir()?;
        let typst_path = output_dir.path().join("expected.typ");
        let processor = Processor::new(Options::default(), doc.attributes.clone())
            .with_pdf_options(PdfOptions {
                emit_typst: Some(typst_path.clone()),
                ..PdfOptions::default()
            });
        let mut pdf = Vec::new();
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("pdf");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        processor.write_to(doc, &mut pdf, None, None, &mut diagnostics)?;
        output.extend(std::fs::read(typst_path)?);
        Ok(())
    })
}
