use std::path::PathBuf;

use acdc_converters_common::{
    Options as ConverterOptions, Processable, output::remove_lines_trailing_whitespace,
};
use acdc_converters_terminal::Processor;
use acdc_parser::Options as ParserOptions;

type Error = Box<dyn std::error::Error>;

macro_rules! generate_tests {
    ( [ $( ($name:ident, $uses_osc8_links:expr) ),* $(,)? ] ) => {
        $(
            #[cfg(test)]
            mod $name {
                use super::*;
                #[test]
                fn test() -> Result<(), Error> {
                    let fixture_name = stringify!($name);
                    test_fixture(fixture_name, $uses_osc8_links)
                }
            }
        )*
    };
}

// List of test fixtures: (fixture_name, uses_osc8_links)
generate_tests!([
    (document, false),
    (nested_sections, false),
    (ordered_list, false),
    (unordered_list, false),
    (description_list_mixed_content, false),
    (table_multi_cell_per_line, false),
    (delimited_block, false),
    (quote_block_with_paragraphs, false),
    (admonition_block, false),
    (footnotes, false),
    (url_macro, true),
    (basic_image_block, false),
    (source_block_with_attribute_in_title, false),
    (source_block_complete, false),
    (macros_with_quoted_attributes, false),
    (escaped_superscript_subscript, false),
    (styled_paragraphs, false),
]);

/// Helper function to run a single integration test.
///
/// Parses the input `.adoc` file, converts to Terminal output, and compares with expected output.
fn test_fixture(fixture_name: &str, osc8: bool) -> Result<(), Error> {
    let input_path = PathBuf::from("tests/fixtures/source").join(format!("{fixture_name}.adoc"));

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
    let expected_normalized = remove_lines_trailing_whitespace(&expected);
    let actual_normalized = remove_lines_trailing_whitespace(&actual);

    pretty_assertions::assert_eq!(
        expected_normalized,
        actual_normalized,
        "Terminal output mismatch for fixture: {fixture_name}",
    );

    Ok(())
}
