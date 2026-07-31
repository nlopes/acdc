use std::path::{Path, PathBuf};

use acdc_converters_core::{Converter, Options as ConverterOptions};
use acdc_converters_pdf::{PdfOptions, Processor};
use acdc_parser::Options as ParserOptions;

type Error = Box<dyn std::error::Error>;

fn run_typst_fixture(path: &Path) -> Result<(), Error> {
    let file_name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or("invalid fixture file name")?;

    #[cfg(not(feature = "pre-spec-subs"))]
    if file_name.contains("subs") {
        return Ok(());
    }

    let expected_path = Path::new("tests/fixtures/expected")
        .join(file_name)
        .with_extension("typ");
    let bootstrap = Processor::new(
        ConverterOptions::default(),
        acdc_converters_core::default_rendering_attributes(),
    );
    let parser_options = ParserOptions::with_attributes(bootstrap.document_attributes().clone());
    let parsed = acdc_parser::parse_file(path, &parser_options)?;
    let output_dir = tempfile::tempdir()?;
    let typst_path = output_dir.path().join("actual.typ");
    let processor = Processor::new(
        ConverterOptions::default(),
        parsed.document().attributes.clone(),
    )
    .with_pdf_options(PdfOptions {
        emit_typst: Some(typst_path.clone()),
        ..PdfOptions::default()
    });
    let mut pdf = Vec::new();
    let mut warnings = Vec::new();
    let source = acdc_converters_core::WarningSource::new("pdf");
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
    processor.write_to(
        parsed.document(),
        &mut pdf,
        Some(path),
        None,
        &mut diagnostics,
    )?;

    assert!(pdf.starts_with(b"%PDF-"));
    let expected = std::fs::read_to_string(expected_path)?;
    let actual = std::fs::read_to_string(typst_path)?;
    pretty_assertions::assert_eq!(
        expected,
        actual,
        "Typst output mismatch for fixture: {file_name}",
    );
    Ok(())
}

#[rstest::rstest]
fn typst_fixtures(#[files("tests/fixtures/source/*.adoc")] path: PathBuf) -> Result<(), Error> {
    run_typst_fixture(&path)
}
