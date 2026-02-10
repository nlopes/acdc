use std::path::{Path, PathBuf};

use acdc_converters_core::{Converter, GeneratorMetadata, Options as ConverterOptions};
use acdc_converters_dev::output::remove_lines_trailing_whitespace;
use acdc_converters_markdown::{MarkdownVariant, Processor};
use acdc_parser::Options as ParserOptions;

type Error = Box<dyn std::error::Error>;

/// Parses the input `.adoc` file, converts to Markdown (GFM), and compares with expected output.
/// Excludes commonmark_* files which have their own test function.
#[rstest::rstest]
#[tracing_test::traced_test]
fn test_gfm_fixtures(#[files("tests/fixtures/source/*.adoc")] path: PathBuf) -> Result<(), Error> {
    let file_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Invalid fixture file name")?;

    // Skip commonmark_* files - they have their own test
    if file_name.starts_with("commonmark_") {
        return Ok(());
    }
    let expected_path = Path::new("tests")
        .join("fixtures")
        .join("expected")
        .join(file_name)
        .with_extension("md");

    // Parse the AsciiDoc input with rendering defaults
    let parser_options =
        ParserOptions::with_attributes(acdc_converters_core::default_rendering_attributes());
    let doc = acdc_parser::parse_file(&path, &parser_options)?;

    // Convert to Markdown (GFM variant)
    let mut output = Vec::new();
    let converter_options = ConverterOptions::builder()
        .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
        .build();
    let processor = Processor::new(converter_options, doc.attributes.clone())
        .with_variant(MarkdownVariant::GitHubFlavored);
    processor.write_to(&doc, &mut output, Some(&path))?;

    // Read expected output
    let expected = std::fs::read_to_string(&expected_path)?;

    // Compare (with normalization)
    let actual = String::from_utf8(output)?;
    let expected_normalized = remove_lines_trailing_whitespace(&expected);
    let actual_normalized = remove_lines_trailing_whitespace(&actual);

    pretty_assertions::assert_eq!(
        expected_normalized,
        actual_normalized,
        "Markdown output mismatch for fixture: {file_name}",
    );
    Ok(())
}

/// Test `CommonMark` variant separately for features that differ
#[rstest::rstest]
#[tracing_test::traced_test]
fn test_commonmark_variant(
    #[files("tests/fixtures/source/commonmark_*.adoc")] path: PathBuf,
) -> Result<(), Error> {
    let file_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Invalid fixture file name")?;
    let expected_path = Path::new("tests")
        .join("fixtures")
        .join("expected")
        .join(file_name)
        .with_extension("md");

    // Parse the AsciiDoc input
    let parser_options =
        ParserOptions::with_attributes(acdc_converters_core::default_rendering_attributes());
    let doc = acdc_parser::parse_file(&path, &parser_options)?;

    // Convert to Markdown (CommonMark variant)
    let mut output = Vec::new();
    let converter_options = ConverterOptions::builder()
        .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
        .build();
    let processor = Processor::new(converter_options, doc.attributes.clone())
        .with_variant(MarkdownVariant::CommonMark);
    processor.write_to(&doc, &mut output, Some(&path))?;

    // Read expected output
    let expected = std::fs::read_to_string(&expected_path)?;

    // Compare (with normalization)
    let actual = String::from_utf8(output)?;
    let expected_normalized = remove_lines_trailing_whitespace(&expected);
    let actual_normalized = remove_lines_trailing_whitespace(&actual);

    pretty_assertions::assert_eq!(
        expected_normalized,
        actual_normalized,
        "CommonMark output mismatch for fixture: {file_name}",
    );
    Ok(())
}
