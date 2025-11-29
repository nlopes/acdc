use std::path::{Path, PathBuf};

use acdc_converters_common::{Options as ConverterOptions, Processable};
use acdc_html::{Processor, RenderOptions};
use acdc_parser::Options as ParserOptions;

type Error = Box<dyn std::error::Error>;

/// Normalizes HTML output for comparison.
///
/// This removes trailing whitespace and normalizes line endings.
fn normalize_html(html: &str) -> String {
    html.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parses the input `.adoc` file, converts to HTML, and compares with expected output.
#[rstest::rstest]
#[tracing_test::traced_test]
fn test_with_fixtures(#[files("tests/fixtures/source/*.adoc")] path: PathBuf) -> Result<(), Error> {
    let file_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Invalid fixture file name")?;
    let expected_path = Path::new("tests")
        .join("fixtures")
        .join("expected")
        .join(file_name)
        .with_extension("html");

    // Parse the `AsciiDoc` input with rendering defaults
    let parser_options = ParserOptions {
        document_attributes: acdc_converters_common::default_rendering_attributes(),
        ..Default::default()
    };
    let doc = acdc_parser::parse_file(&path, &parser_options)?;

    // Convert to HTML
    let mut output = Vec::new();
    let converter_options = ConverterOptions {
        generator_metadata: acdc_converters_common::GeneratorMetadata::new("acdc", "0.1.0"),
        ..Default::default()
    };
    let processor = Processor::new(converter_options, doc.attributes.clone());
    let render_options = RenderOptions::default();
    processor.convert_to_writer(&doc, &mut output, &render_options)?;

    // Read expected output
    let expected = std::fs::read_to_string(&expected_path)?;

    // Compare (with normalization)
    let actual = String::from_utf8(output)?;
    let expected_normalized = normalize_html(&expected);
    let actual_normalized = normalize_html(&actual);

    pretty_assertions::assert_eq!(
        expected_normalized,
        actual_normalized,
        "HTML output mismatch for fixture: {file_name}",
    );
    Ok(())
}
