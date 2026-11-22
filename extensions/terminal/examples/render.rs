//! Select the terminal extension explicitly and render through the shared pipeline.

use std::error::Error;

use acdc_converters_core::{
    Converter as _, Diagnostics, Options,
    pipeline::{ExtensionCatalog, Pipeline},
};
use acdc_converters_html::Converter;
use acdc_parser::{Options as ParserOptions, SafeMode};

fn main() -> Result<(), Box<dyn Error>> {
    let mut available = ExtensionCatalog::default();
    available.register("terminal", acdc_extensions_terminal::extension())?;
    let selection = available.select(["terminal"])?.ok_or("empty selection")?;
    let pipeline = Pipeline::new(Converter::new(
        Options::builder()
            .embedded(true)
            .safe_mode(SafeMode::Unsafe)
            .build(),
        ParserOptions::builder(),
    )?)
    .with_extensions(selection);
    let parsed = pipeline.parse("[terminal,cols=40,rows=4]\n----\n$ echo hello\nhello\n----")?;
    for warning in parsed.warnings() {
        eprintln!("{warning}");
    }
    let mut warnings = Vec::new();
    let source = pipeline.converter().warning_source();
    pipeline.write_to(
        parsed.document(),
        std::io::stdout().lock(),
        None,
        None,
        &mut Diagnostics::new(&source, &mut warnings),
    )?;
    for warning in warnings {
        eprintln!("{warning}");
    }
    Ok(())
}
