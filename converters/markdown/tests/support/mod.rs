use std::{error::Error, path::Path};

use acdc_converters_core::{
    Converter as _, Diagnostics, GeneratorMetadata, Options, WarningSource,
};
use acdc_converters_dev::fixtures::{FixtureOutput, parse_fixture};
use acdc_converters_markdown::{Converter, MarkdownVariant};
use acdc_parser::{DocumentAttributes, Options as ParserOptions, SafeMode};

pub(crate) fn render(path: &Path) -> Result<FixtureOutput, Box<dyn Error>> {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or("invalid Markdown fixture stem")?;
    let variant = if stem.starts_with("commonmark_") {
        MarkdownVariant::CommonMark
    } else {
        MarkdownVariant::GitHubFlavored
    };
    let parsed = parse_fixture(path)?;
    let doc = parsed.document();
    let converter = Converter::new(
        Options::builder()
            .safe_mode(SafeMode::Unsafe)
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .build(),
        ParserOptions::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_attributes(DocumentAttributes::from(doc.attributes()).into_inputs()),
    )?
    .with_variant(variant);
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let source = WarningSource::new("markdown").with_variant(variant.as_str());
    converter.write_to(
        doc,
        &mut output,
        Some(path),
        None,
        &mut Diagnostics::new(&source, &mut warnings),
    )?;
    Ok(FixtureOutput { output, warnings })
}
