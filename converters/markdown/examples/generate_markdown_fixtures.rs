//! Generate expected Markdown output files for integration tests.

use std::{error::Error, fs, path::Path};

use acdc_converters_core::{Converter, Diagnostics, GeneratorMetadata, Options, WarningSource};
use acdc_converters_markdown::{MarkdownVariant, Processor};
use acdc_parser::{DocumentAttributes, Options as ParserOptions};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let requested_fixture = arguments.next();
    if arguments.next().is_some() {
        return Err("usage: generate_markdown_fixtures [fixture-name]".into());
    }

    let source_dir = Path::new("converters/markdown/tests/fixtures/source");
    let expected_dir = Path::new("converters/markdown/tests/fixtures/expected");
    let mut fixtures = source_dir
        .read_dir()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "adoc")
                && requested_fixture
                    .as_deref()
                    .is_none_or(|requested| path.file_stem() == Some(requested))
        })
        .collect::<Vec<_>>();
    fixtures.sort();

    if fixtures.is_empty()
        && let Some(requested) = requested_fixture
    {
        return Err(format!("unknown Markdown fixture: {}", requested.to_string_lossy()).into());
    }

    for input_path in fixtures {
        let stem = input_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or("invalid fixture file name")?;
        let parser_options = ParserOptions::with_attributes(DocumentAttributes::default());
        let parsed = acdc_parser::parse_file(&input_path, &parser_options)?;
        let doc = parsed.document();
        let variant = if stem.starts_with("commonmark_") {
            MarkdownVariant::CommonMark
        } else {
            MarkdownVariant::GitHubFlavored
        };
        let options = Options::builder()
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .build();
        let processor = Processor::new(options, doc.attributes.clone()).with_variant(variant);
        let mut output = Vec::new();
        let mut warnings = Vec::new();
        let source = WarningSource::new("markdown").with_variant(variant.as_str());
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        processor.write_to(doc, &mut output, Some(&input_path), None, &mut diagnostics)?;
        fs::write(expected_dir.join(stem).with_extension("md"), output)?;
    }

    Ok(())
}
