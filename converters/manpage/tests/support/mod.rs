use std::{error::Error, path::Path};

use acdc_converters_core::{Converter, Diagnostics, GeneratorMetadata, Options, WarningSource};
use acdc_converters_dev::fixtures::{FixtureOutput, parse_fixture};
use acdc_converters_manpage::Processor;
use acdc_parser::{DocumentAttributes, Options as ParserOptions, SafeMode};

pub(crate) fn render(path: &Path, embedded: bool) -> Result<FixtureOutput, Box<dyn Error>> {
    let parsed = parse_fixture(path)?;
    let doc = parsed.document();
    let processor = Processor::new(
        Options::builder()
            .safe_mode(SafeMode::Unsafe)
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .embedded(embedded)
            .build(),
        ParserOptions::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_attributes(DocumentAttributes::from(doc.attributes()).into_inputs()),
    )?;
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let source = WarningSource::new("manpage");
    processor.write_to(
        doc,
        &mut output,
        Some(path),
        None,
        &mut Diagnostics::new(&source, &mut warnings),
    )?;
    Ok(FixtureOutput { output, warnings })
}
