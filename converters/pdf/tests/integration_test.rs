use std::path::{Path, PathBuf};

use acdc_converters_core::{Converter, Options as ConverterOptions};
use acdc_converters_pdf::{PdfOptions, Processor};
use acdc_parser::Options as ParserOptions;

type Error = Box<dyn std::error::Error>;

fn fixture_theme(doc: &acdc_parser::Document<'_>) -> Option<PathBuf> {
    doc.attributes
        .get_string("acdc-pdf-test-theme")
        .map(|name| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/themes")
                .join(name.as_ref())
                .with_extension("yaml")
        })
}

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
        theme: fixture_theme(parsed.document()),
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
    let minimum_pages_path = expected_path.with_extension("min-pages");
    if minimum_pages_path.exists() {
        let minimum_pages = std::fs::read_to_string(&minimum_pages_path)?
            .trim()
            .parse::<usize>()?;
        let rendered = lopdf::Document::load_mem(&pdf)?;
        let actual_pages = rendered.get_pages().len();
        assert!(
            actual_pages >= minimum_pages,
            "PDF page count for {file_name} is {actual_pages}; expected at least {minimum_pages}",
        );
    }
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

#[test]
fn image_alt_text_reaches_pdf_structure() -> Result<(), Error> {
    let path = Path::new("tests/fixtures/source/image_accessibility_alt_text.adoc");
    let bootstrap = Processor::new(
        ConverterOptions::default(),
        acdc_converters_core::default_rendering_attributes(),
    );
    let parser_options = ParserOptions::with_attributes(bootstrap.document_attributes().clone());
    let parsed = acdc_parser::parse_file(path, &parser_options)?;
    let processor = Processor::new(
        ConverterOptions::default(),
        parsed.document().attributes.clone(),
    );
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

    let rendered = lopdf::Document::load_mem(&pdf)?;
    let mut descriptions = rendered
        .objects
        .values()
        .filter_map(|object| {
            let dictionary = object.as_dict().ok()?;
            let alt = dictionary.get(b"Alt").ok()?;
            lopdf::decode_text_string(alt).ok()
        })
        .collect::<Vec<_>>();
    descriptions.sort();

    assert_eq!(
        descriptions,
        [
            "Explicit block description",
            "Explicit inline description",
            "Linked description",
            "Positioned description",
            "inline image dimensions",
            "inline image dimensions",
        ]
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    Ok(())
}
