use std::{error::Error, path::Path};

use acdc_converters_core::{Converter, Diagnostics, Options, WarningSource};
use acdc_converters_dev::fixtures::{FixtureOutput, parse_fixture};
use acdc_converters_terminal::{Capabilities, Processor};
use acdc_parser::{DocumentAttributes, Options as ParserOptions, SafeMode};

pub(crate) fn render(path: &Path, osc8_links: bool) -> Result<FixtureOutput, Box<dyn Error>> {
    crossterm::style::force_color_output(true);
    let parsed = parse_fixture(path)?;
    let doc = parsed.document();
    let processor = Processor::new(
        Options::builder().safe_mode(SafeMode::Unsafe).build(),
        ParserOptions::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_attributes(DocumentAttributes::from(doc.attributes()).into_inputs()),
    )?
    .with_terminal_width(80)
    .with_dark_mode(true)
    .with_terminal_capabilities(Capabilities {
        unicode: true,
        osc8_links,
    });
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let source = WarningSource::new("terminal");
    processor.write_to(
        doc,
        &mut output,
        Some(path),
        None,
        &mut Diagnostics::new(&source, &mut warnings),
    )?;
    Ok(FixtureOutput { output, warnings })
}
