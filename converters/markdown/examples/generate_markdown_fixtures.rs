//! Generate expected Markdown output files for integration tests.

use std::{error::Error, fs, path::Path};

use acdc_converters_core::{Converter, Diagnostics, GeneratorMetadata, Options, WarningSource};
use acdc_converters_markdown::{MarkdownVariant, Processor};

fn main() -> Result<(), Box<dyn Error>> {
    let source_dir = Path::new("converters/markdown/tests/fixtures/source");
    let expected_dir = Path::new("converters/markdown/tests/fixtures/expected");
    let mut fixtures = source_dir
        .read_dir()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "adoc")
        })
        .collect::<Vec<_>>();
    fixtures.sort();

    for input_path in fixtures {
        let stem = input_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or("invalid fixture file name")?;
        let parser_options = acdc_parser::Options::with_attributes(
            acdc_converters_core::default_rendering_attributes(),
        );
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
