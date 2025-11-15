use std::path::PathBuf;

use acdc_converters_common::{Options as ConverterOptions, Processable};
use acdc_parser::Options as ParserOptions;
use acdc_terminal::Processor;

type Error = Box<dyn std::error::Error>;

/// Normalizes terminal output for comparison.
///
/// This removes trailing whitespace and normalizes line endings.
fn normalize_output(output: &str) -> String {
    output
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Helper function to run a single integration test.
///
/// Parses the input `.adoc` file, converts to Terminal output, and compares with expected output.
fn test_fixture(fixture_name: &str, osc8: bool) -> Result<(), Error> {
    let input_path =
        PathBuf::from("../../acdc-parser/fixtures/tests").join(format!("{fixture_name}.adoc"));

    // Parse the `AsciiDoc` input with rendering defaults
    let parser_options = ParserOptions {
        document_attributes: acdc_converters_common::default_rendering_attributes(),
        ..Default::default()
    };
    let doc = acdc_parser::parse_file(&input_path, &parser_options)?;

    // Convert to Terminal output
    let mut output = Vec::new();
    let processor = Processor::new(ConverterOptions::default(), doc.attributes.clone());
    processor.convert_to_writer(&doc, &mut output)?;

    if osc8 && !processor.appearance.capabilities.osc8_links {
        // If the fixture name indicates osc8 links but we're running in a terminal that
        // doesn't support them, we ignore the test.
        return Ok(());
    }
    let fixture_name = if osc8 {
        format!("{fixture_name}.osc8.txt")
    } else {
        format!("{fixture_name}.txt")
    };
    let expected_path = PathBuf::from("tests/fixtures/expected").join(&fixture_name);

    // Read expected output
    let expected = std::fs::read_to_string(&expected_path)?;

    // Compare (with normalization)
    let actual = String::from_utf8(output)?;
    let expected_normalized = normalize_output(&expected);
    let actual_normalized = normalize_output(&actual);

    pretty_assertions::assert_eq!(
        expected_normalized,
        actual_normalized,
        "Terminal output mismatch for fixture: {fixture_name}",
    );

    Ok(())
}

#[test]
fn test_document() -> Result<(), Error> {
    test_fixture("document", false)
}

#[test]
fn test_nested_sections() -> Result<(), Error> {
    test_fixture("nested_sections", false)
}

#[test]
fn test_ordered_list() -> Result<(), Error> {
    test_fixture("ordered_list", false)
}

#[test]
fn test_unordered_list() -> Result<(), Error> {
    test_fixture("unordered_list", false)
}

#[test]
fn test_description_list_mixed_content() -> Result<(), Error> {
    test_fixture("description_list_mixed_content", false)
}

#[test]
fn test_table_multi_cell_per_line() -> Result<(), Error> {
    test_fixture("table_multi_cell_per_line", false)
}

#[test]
fn test_delimited_block() -> Result<(), Error> {
    test_fixture("delimited_block", false)
}

#[test]
fn test_quote_block_with_paragraphs() -> Result<(), Error> {
    test_fixture("quote_block_with_paragraphs", false)
}

#[test]
fn test_admonition_block() -> Result<(), Error> {
    test_fixture("admonition_block", false)
}

#[test]
fn test_footnotes() -> Result<(), Error> {
    test_fixture("footnotes", false)
}

#[test]
fn test_url_macro() -> Result<(), Error> {
    test_fixture("url_macro", true)
}

#[test]
fn test_basic_image_block() -> Result<(), Error> {
    test_fixture("basic_image_block", false)
}

#[test]
fn test_source_block_with_attribute_in_title() -> Result<(), Error> {
    test_fixture("source_block_with_attribute_in_title", false)
}

#[test]
fn test_source_block_complete() -> Result<(), Error> {
    test_fixture("source_block_complete", false)
}

#[test]
fn test_macros_with_quoted_attributes() -> Result<(), Error> {
    test_fixture("macros_with_quoted_attributes", false)
}
