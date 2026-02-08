use std::path::{Path, PathBuf};

use acdc_converters_core::{Backend, GeneratorMetadata, Options as ConverterOptions};
use acdc_converters_dev::output::remove_lines_trailing_whitespace;
use acdc_converters_html::{HtmlVariant, Processor, RenderOptions};
use acdc_parser::Options as ParserOptions;

type Error = Box<dyn std::error::Error>;

fn run_fixture_test(
    path: &Path,
    expected_dir: &Path,
    variant: HtmlVariant,
    embedded: bool,
) -> Result<(), Error> {
    let file_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Invalid fixture file name")?;
    let expected_path = expected_dir.join(file_name).with_extension("html");

    let parser_options =
        ParserOptions::with_attributes(acdc_converters_core::default_rendering_attributes());
    let doc = acdc_parser::parse_file(path, &parser_options)?;

    let backend = match variant {
        HtmlVariant::Semantic => Backend::Html5s,
        HtmlVariant::Standard => Backend::Html,
    };
    let converter_options = ConverterOptions::builder()
        .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
        .backend(backend)
        .build();
    let processor = Processor::new_with_variant(converter_options, doc.attributes.clone(), variant);
    let render_options = RenderOptions {
        embedded,
        ..RenderOptions::default()
    };

    let mut output = Vec::new();
    processor.convert_to_writer(&doc, &mut output, &render_options)?;

    let expected = std::fs::read_to_string(&expected_path)?;
    let actual = String::from_utf8(output)?;
    let expected_normalized = remove_lines_trailing_whitespace(&expected);
    let actual_normalized = remove_lines_trailing_whitespace(&actual);

    pretty_assertions::assert_eq!(
        expected_normalized,
        actual_normalized,
        "HTML output mismatch for fixture: {file_name}",
    );
    Ok(())
}

#[rstest::rstest]
#[tracing_test::traced_test]
fn test_with_fixtures(#[files("tests/fixtures/source/*.adoc")] path: PathBuf) -> Result<(), Error> {
    run_fixture_test(
        &path,
        Path::new("tests/fixtures/expected"),
        HtmlVariant::Standard,
        false,
    )
}

#[rstest::rstest]
#[tracing_test::traced_test]
fn test_html5s_with_fixtures(
    #[files("tests/fixtures/source/html5s/*.adoc")] path: PathBuf,
) -> Result<(), Error> {
    run_fixture_test(
        &path,
        Path::new("tests/fixtures/expected/html5s"),
        HtmlVariant::Semantic,
        true,
    )
}
