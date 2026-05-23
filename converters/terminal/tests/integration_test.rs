use std::path::PathBuf;

use acdc_converters_core::{Converter, Options as ConverterOptions};
use acdc_converters_dev::output::remove_lines_trailing_whitespace;
use acdc_converters_terminal::Processor;
use acdc_parser::Options as ParserOptions;

type Error = Box<dyn std::error::Error>;

#[cfg(feature = "images")]
fn temp_output_path(name: &str, extension: &str) -> PathBuf {
    std::env::temp_dir().join(format!("acdc-{name}-{}.{extension}", std::process::id()))
}

/// Generate a `#[test]` for each fixture. The optional `requires:` clause
/// gates the test on a cfg predicate so fixtures whose expected output
/// depends on a specific terminal feature (e.g. `images`, `highlighting`)
/// are simply absent from the test run when that feature is off, rather
/// than failing with a stale expected output.
macro_rules! generate_tests {
    ( [ $( ($name:ident, $uses_osc8_links:expr $(, requires: $cfg:meta )? ) ),* $(,)? ] ) => {
        $(
            $( #[cfg($cfg)] )?
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

// List of test fixtures: (fixture_name, uses_osc8_links [, requires: <cfg>])
generate_tests!([
    (document, false),
    (nested_sections, false),
    (ordered_list, false),
    (unordered_list, false),
    (description_list_mixed_content, false),
    (table_multi_cell_per_line, false),
    (table_cell_colspan, false),
    (table_cell_rowspan, false),
    (table_cell_span_combined, false),
    (delimited_block, false),
    (quote_block_with_paragraphs, false),
    (admonition_block, false),
    (footnotes, false),
    (url_macro, true),
    (basic_image_block, false, requires: feature = "images"),
    (source_block_with_attribute_in_title, false, requires: feature = "highlighting"),
    (source_block_complete, false, requires: feature = "highlighting"),
    (macros_with_quoted_attributes, false, requires: feature = "images"),
    (escaped_superscript_subscript, false),
    (styled_paragraphs, false),
    (comprehensive, true, requires: all(feature = "images", feature = "highlighting")),
    (index_section, false),
    (subs_replacements_disabled, false),
    (subs_replacements_explicit, false),
]);

/// Helper function to run a single integration test.
///
/// Parses the input `.adoc` file, converts to Terminal output, and compares with expected output.
fn test_fixture(fixture_name: &str, osc8: bool) -> Result<(), Error> {
    // Fixtures whose name contains `subs` test `[subs="…"]` behaviour, which
    // only takes effect under the `pre-spec-subs` feature. When the feature
    // is off, skip — the expected output captures the feature-on behaviour
    // and cannot match.
    #[cfg(not(feature = "pre-spec-subs"))]
    if fixture_name.contains("subs") {
        return Ok(());
    }

    crossterm::style::force_color_output(true);

    let input_path = PathBuf::from("tests/fixtures/source").join(format!("{fixture_name}.adoc"));

    // Parse the `AsciiDoc` input with rendering defaults
    let parser_options =
        ParserOptions::with_attributes(acdc_converters_core::default_rendering_attributes());
    let parsed = acdc_parser::parse_file(&input_path, &parser_options)?;
    let doc = parsed.document();

    // Convert to Terminal output
    let mut output = Vec::new();
    let processor =
        Processor::new(ConverterOptions::default(), doc.attributes.clone()).with_terminal_width(80);
    let mut warnings = Vec::new();
    let source = acdc_converters_core::WarningSource::new("terminal");
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
    processor.write_to(
        doc,
        &mut output,
        Some(input_path.as_path()),
        None,
        &mut diagnostics,
    )?;

    if osc8 && !processor.terminal_capabilities().osc8_links {
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

#[cfg(feature = "images")]
#[test]
fn image_failure_warning_is_returned_in_conversion_result() -> Result<(), Error> {
    let parser_options =
        ParserOptions::with_attributes(acdc_converters_core::default_rendering_attributes());
    let parsed = acdc_parser::parse("image::definitely-missing-image.png[]\n", &parser_options)?;
    let doc = parsed.document();
    let processor =
        Processor::new(ConverterOptions::default(), doc.attributes.clone()).with_terminal_width(80);
    let output_path = temp_output_path("terminal-warning", "txt");

    let result = processor.convert_to_file(doc, None, &output_path)?;
    let _ = std::fs::remove_file(&output_path);

    assert!(result.warnings().iter().any(|warning| {
        warning.source.converter == "terminal"
            && warning.message.contains("definitely-missing-image.png")
    }));
    Ok(())
}
