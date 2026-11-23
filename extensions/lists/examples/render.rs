//! Explicit native lists registration through the shared pipeline.

use std::{error::Error, path::Path};

use acdc_converters_core::{
    Converter as _, Diagnostics, Options,
    pipeline::{ExtensionCatalog, Pipeline},
};
use acdc_converters_html::Converter;
use acdc_parser::Options as ParserOptions;

fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args_os().nth(1).ok_or("provide a source file")?;
    let mut available = ExtensionCatalog::default();
    available.register("lists", acdc_extensions_lists::registry()?)?;
    let selected = available.select(["lists"])?.ok_or("empty selection")?;
    let pipeline = Pipeline::new(Converter::new(
        Options::builder().embedded(true).build(),
        ParserOptions::builder(),
    )?)
    .with_extensions(selected);
    let parsed = pipeline.parse_file(Path::new(&path))?;
    for warning in parsed.warnings() {
        eprintln!("{warning}");
    }
    let source = pipeline.converter().warning_source();
    let mut warnings = Vec::new();
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
