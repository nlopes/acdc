use acdc_parser::{Options, parse_file};
use lopdf::{Document as PdfDocument, decode_text_string};
use std::{
    fs::read_to_string,
    path::{Path, PathBuf},
};

use acdc_converters_core::{Converter, Diagnostics, Options as ConverterOptions, WarningSource};
use acdc_converters_pdf::{PdfOptions, Processor};

type Error = Box<dyn std::error::Error>;

fn fixture_theme(doc: &acdc_parser::Document<'_>) -> Option<PathBuf> {
    doc.attributes
        .get("acdc-pdf-test-theme")
        .and_then(|value| value.text())
        .map(|name| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/themes")
                .join(name)
                .with_extension("yaml")
        })
}

fn assert_repeated_table_header_index(pdf: &[u8]) -> Result<(), Error> {
    let rendered = PdfDocument::load_mem(pdf)?;
    let pages = rendered.get_pages().keys().copied().collect::<Vec<_>>();
    let mut repeated_header_pages = Vec::new();
    for page in &pages {
        let text = rendered.extract_text(&[*page])?;
        let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized.contains("Description with visible header") {
            repeated_header_pages.push(*page);
        }
    }
    assert!(
        repeated_header_pages.len() >= 2,
        "expected a repeated table header, found it on pages {repeated_header_pages:?}",
    );
    let last_header_page = repeated_header_pages
        .last()
        .copied()
        .ok_or("repeated table header page not found")?;
    let text = rendered.extract_text(&pages)?;
    let normalized = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" , ", ", ");
    for term in ["Related header", "shared term", "visible header"] {
        let expected = format!("{term}, {last_header_page}");
        assert!(
            normalized.contains(&expected),
            "expected index entry `{expected}` in PDF text:\n{text}",
        );
    }
    Ok(())
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
    let bootstrap = Processor::new(ConverterOptions::default(), Options::builder())?;
    let parsed = parse_file(path, bootstrap.parser_options())?;
    let output_dir = tempfile::tempdir()?;
    let typst_path = output_dir.path().join("actual.typ");
    let processor = Processor::new(
        ConverterOptions::default(),
        Options::builder().with_attributes(parsed.document().attributes.clone().into_inputs()),
    )?
    .with_pdf_options(PdfOptions {
        emit_typst: Some(typst_path.clone()),
        theme: fixture_theme(parsed.document()),
        ..PdfOptions::default()
    });
    let mut pdf = Vec::new();
    let mut warnings = Vec::new();
    let source = WarningSource::new("pdf");
    let mut diagnostics = Diagnostics::new(&source, &mut warnings);
    processor.write_to(
        parsed.document(),
        &mut pdf,
        Some(path),
        None,
        &mut diagnostics,
    )?;

    if file_name == "silent_metadata" {
        assert_eq!(
            warnings
                .iter()
                .map(|warning| warning.message.as_ref())
                .collect::<Vec<_>>(),
            [
                "inline image `fit=none` page-height sizing is not supported by the PDF backend; rendering with normal intrinsic sizing",
                "PHP source block mixed-mode highlighting is not supported by the PDF backend; rendering with Typst's normal PHP highlighter",
                "page-break layout changes are not supported by the PDF backend; keeping the document page layout",
            ]
        );
        assert!(warnings.iter().all(|warning| warning.advice.is_some()));
        assert_eq!(
            warnings
                .iter()
                .filter_map(|warning| warning.source_location())
                .map(|location| location.location.start.line)
                .collect::<Vec<_>>(),
            [97, 101, 118]
        );
    }

    if file_name == "parity_kitchen_sink" {
        assert!(warnings.is_empty(), "{warnings:?}");
        let rendered = PdfDocument::load_mem(&pdf)?;
        let (_, info) = rendered.dereference(rendered.trailer.get(b"Info")?)?;
        let info = info.as_dict()?;
        for (key, expected) in [
            (b"Title".as_slice(), "PDF Parity Kitchen Sink: API Coverage"),
            (b"Author".as_slice(), "Ada Lovelace, Grace B. Hopper"),
            (
                b"Subject".as_slice(),
                "Representative PDF converter coverage",
            ),
            (b"Keywords".as_slice(), "parity, PDF, converter API"),
        ] {
            assert_eq!(decode_text_string(info.get(key)?)?, expected);
        }
    }

    if file_name.starts_with("index_terms_repeated_table_header") {
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_repeated_table_header_index(&pdf)?;
    }

    assert!(pdf.starts_with(b"%PDF-"));
    let minimum_pages_path = expected_path.with_extension("min-pages");
    if minimum_pages_path.exists() {
        let minimum_pages = read_to_string(&minimum_pages_path)?
            .trim()
            .parse::<usize>()?;
        let rendered = PdfDocument::load_mem(&pdf)?;
        let actual_pages = rendered.get_pages().len();
        assert!(
            actual_pages >= minimum_pages,
            "PDF page count for {file_name} is {actual_pages}; expected at least {minimum_pages}",
        );
    }
    let expected = read_to_string(expected_path)?;
    let actual = read_to_string(typst_path)?;
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
fn link_macro_ids_are_named_pdf_destinations() -> Result<(), Error> {
    let path = Path::new("tests/fixtures/source/link_macro_ids.adoc");
    let bootstrap = Processor::new(ConverterOptions::default(), Options::builder())?;
    let parsed = parse_file(path, bootstrap.parser_options())?;
    let processor = Processor::new(
        ConverterOptions::default(),
        Options::builder().with_attributes(parsed.document().attributes.clone().into_inputs()),
    )?;
    let mut pdf = Vec::new();
    let mut warnings = Vec::new();
    let source = WarningSource::new("pdf");
    let mut diagnostics = Diagnostics::new(&source, &mut warnings);
    processor.write_to(
        parsed.document(),
        &mut pdf,
        Some(path),
        None,
        &mut diagnostics,
    )?;

    let rendered = PdfDocument::load_mem(&pdf)?;
    let (_, names) = rendered.dereference(rendered.catalog()?.get(b"Names")?)?;
    let (_, destinations) = rendered.dereference(names.as_dict()?.get(b"Dests")?)?;
    let (_, entries) = rendered.dereference(destinations.as_dict()?.get(b"Names")?)?;
    let mut names = entries
        .as_array()?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|[name, _]| decode_text_string(name))
        .collect::<Result<Vec<_>, _>>()?;
    names.sort();

    assert_eq!(
        names,
        [
            "bare-link-id",
            "bare-mailto-id",
            "duplicate-id",
            "link-id",
            "mailto-id",
            "url-id",
        ]
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    Ok(())
}

#[test]
fn image_alt_text_reaches_pdf_structure() -> Result<(), Error> {
    let path = Path::new("tests/fixtures/source/image_accessibility_alt_text.adoc");
    let bootstrap = Processor::new(ConverterOptions::default(), Options::builder())?;
    let parsed = parse_file(path, bootstrap.parser_options())?;
    let processor = Processor::new(
        ConverterOptions::default(),
        Options::builder().with_attributes(parsed.document().attributes.clone().into_inputs()),
    )?;
    let mut pdf = Vec::new();
    let mut warnings = Vec::new();
    let source = WarningSource::new("pdf");
    let mut diagnostics = Diagnostics::new(&source, &mut warnings);
    processor.write_to(
        parsed.document(),
        &mut pdf,
        Some(path),
        None,
        &mut diagnostics,
    )?;

    let rendered = PdfDocument::load_mem(&pdf)?;
    let mut descriptions = rendered
        .objects
        .values()
        .filter_map(|object| {
            let dictionary = object.as_dict().ok()?;
            let alt = dictionary.get(b"Alt").ok()?;
            decode_text_string(alt).ok()
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
