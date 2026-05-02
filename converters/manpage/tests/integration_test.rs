use std::path::{Path, PathBuf};

use acdc_converters_core::{Converter, GeneratorMetadata, Options as ConverterOptions};
use acdc_converters_dev::output::remove_lines_trailing_whitespace;
use acdc_converters_manpage::Processor;
use acdc_parser::Options as ParserOptions;

type Error = Box<dyn std::error::Error>;

fn temp_output_path(name: &str, extension: &str) -> PathBuf {
    std::env::temp_dir().join(format!("acdc-{name}-{}.{extension}", std::process::id()))
}

fn run_manpage_fixture(path: &Path, expected_dir: &Path, embedded: bool) -> Result<(), Error> {
    let file_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Invalid fixture file name")?;
    let expected_path = expected_dir.join(file_name).with_extension("man");

    // Parse the `AsciiDoc` input with rendering defaults
    let parser_options =
        ParserOptions::with_attributes(acdc_converters_core::default_rendering_attributes());
    let parsed = acdc_parser::parse_file(path, &parser_options)?;
    let doc = parsed.document();

    // Convert to manpage output
    let mut output = Vec::new();
    let converter_options = ConverterOptions::builder()
        .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
        .embedded(embedded)
        .build();
    let processor = Processor::new(converter_options, doc.attributes.clone());
    let mut warnings = Vec::new();
    let source = acdc_converters_core::WarningSource::new("manpage");
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
    processor.write_to(doc, &mut output, Some(path), None, &mut diagnostics)?;

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

/// Parses the input `.adoc` file, converts to manpage output, and compares with expected output.
#[rstest::rstest]
#[tracing_test::traced_test]
fn test_with_fixtures(#[files("tests/fixtures/source/*.adoc")] path: PathBuf) -> Result<(), Error> {
    run_manpage_fixture(&path, Path::new("tests/fixtures/expected"), false)
}

/// Parses the input `.adoc` file, converts to embedded manpage output, and compares with expected.
#[rstest::rstest]
#[tracing_test::traced_test]
fn test_embedded_with_fixtures(
    #[files("tests/fixtures/source/embedded/*.adoc")] path: PathBuf,
) -> Result<(), Error> {
    run_manpage_fixture(&path, Path::new("tests/fixtures/expected/embedded"), true)
}

#[test]
fn section_order_warning_is_returned_in_conversion_result() -> Result<(), Error> {
    let parser_options =
        ParserOptions::with_attributes(acdc_converters_core::default_rendering_attributes());
    let parsed = acdc_parser::parse(
        "= cmd(1)\n:doctype: manpage\n\n== OVERVIEW\n\ntext\n",
        &parser_options,
    )?;
    let doc = parsed.document();
    let processor = Processor::new(ConverterOptions::default(), doc.attributes.clone());
    let output_path = temp_output_path("manpage-warning", "1");

    let result = processor.convert_to_file(doc, None, &output_path)?;
    let _ = std::fs::remove_file(&output_path);

    assert!(result.warnings().iter().any(|warning| {
        warning.source.converter == "manpage"
            && warning
                .message
                .contains("NAME should be the first section, got `OVERVIEW`")
    }));
    Ok(())
}
