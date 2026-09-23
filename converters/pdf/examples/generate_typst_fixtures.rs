//! Generate expected Typst output files for PDF integration tests.
//!
//! Usage:
//!   `cargo run -p acdc-converters-pdf --example generate_typst_fixtures --all-features`

use acdc_converters_core::{Converter, Options};
use acdc_converters_dev::generate_fixtures::FixtureGenerator;
use acdc_converters_pdf::{PdfOptions, Processor};

fn fixture_theme(doc: &acdc_parser::Document<'_>) -> Option<std::path::PathBuf> {
    doc.attributes
        .get("acdc-pdf-test-theme")
        .and_then(|value| value.text())
        .map(|name| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/themes")
                .join(name)
                .with_extension("yaml")
        })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The test harness parses each fixture with the options the backend
    // settles on, not the parser defaults, so the generator does the same.
    let bootstrap = Processor::new(Options::default(), acdc_parser::Options::builder())?;
    let parser_options = bootstrap.parser_options().clone();
    FixtureGenerator::new("pdf", "typ")
        .with_parser_options(parser_options)
        .generate(|_, doc, output| {
            let source_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/source/fixture.adoc");
            let output_dir = tempfile::tempdir()?;
            let typst_path = output_dir.path().join("expected.typ");
            let processor = Processor::new(
                Options::default(),
                acdc_parser::Options::builder()
                    .with_attributes(doc.attributes.clone().into_inputs()),
            )?
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
