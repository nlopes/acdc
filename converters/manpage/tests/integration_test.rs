use std::path::{Path, PathBuf};

use acdc_converters_core::{Converter, GeneratorMetadata, Options as ConverterOptions};
use acdc_converters_dev::output::remove_lines_trailing_whitespace;
use acdc_converters_manpage::Processor;
use acdc_parser::Options as ParserOptions;

type Error = Box<dyn std::error::Error>;

/// Parses the input `.adoc` file, converts to manpage output, and compares with expected output.
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
        .with_extension("man");

    // Parse the `AsciiDoc` input with rendering defaults
    let parser_options =
        ParserOptions::with_attributes(acdc_converters_core::default_rendering_attributes());
    let doc = acdc_parser::parse_file(&path, &parser_options)?;

    // Convert to manpage output
    let mut output = Vec::new();
    let converter_options = ConverterOptions::builder()
        .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
        .build();
    let processor = Processor::new(converter_options, doc.attributes.clone());
    processor.write_to(&doc, &mut output, Some(path.as_path()))?;

    // Read expected output
    let expected = std::fs::read_to_string(&expected_path)?;

    // Compare (with normalization)
    let actual = String::from_utf8(output)?;
    let expected_normalized = remove_lines_trailing_whitespace(&expected);
    let actual_normalized = remove_lines_trailing_whitespace(&actual);

    pretty_assertions::assert_eq!(
        expected_normalized,
        actual_normalized,
        "Manpage output mismatch for fixture: {file_name}",
    );

    Ok(())
}
