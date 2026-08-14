//! Generate expected Typst output files for PDF integration tests.
//!
//! Usage:
//!   `cargo run -p acdc-converters-pdf --example generate_typst_fixtures --all-features`

use acdc_converters_core::{Converter, Options};
use acdc_converters_dev::generate_fixtures::FixtureGenerator;
use acdc_converters_pdf::{PdfOptions, Processor};

fn fixture_theme(doc: &acdc_parser::Document<'_>) -> Option<std::path::PathBuf> {
    doc.attributes
        .get_string("acdc-pdf-test-theme")
        .map(|name| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/themes")
                .join(name.as_ref())
                .with_extension("yaml")
        })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    FixtureGenerator::new("pdf", "typ").generate(|_, doc, output| {
        let source_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/source/fixture.adoc");
        let output_dir = tempfile::tempdir()?;
        let typst_path = output_dir.path().join("expected.typ");
        let processor = Processor::new(Options::default(), doc.attributes.clone())
            .with_pdf_options(PdfOptions {
                emit_typst: Some(typst_path.clone()),
                theme: fixture_theme(doc),
                ..PdfOptions::default()
            });
        let mut pdf = Vec::new();
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("pdf");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        processor.write_to(doc, &mut pdf, Some(&source_file), None, &mut diagnostics)?;
        output.extend(std::fs::read(typst_path)?);
        Ok(())
    })
}
