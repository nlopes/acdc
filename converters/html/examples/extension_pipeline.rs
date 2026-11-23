//! An application makes registrations available, selects them, then parses and converts.

use std::error::Error;

use acdc_converters_core::{
    Converter as _, Diagnostics, Options,
    pipeline::{ExtensionCatalog, Pipeline},
    registry::ConverterRegistry,
};
use acdc_converters_html::{Converter, REGISTRATION};
use acdc_parser::{
    Options as ParserOptions,
    extensions::{
        BlockMacro, BlockMacroOutput, BlockMacroProcessor, MacroExecutionContext, MacroInvocation,
        Registry,
    },
};

struct Greeting;

impl BlockMacroProcessor for Greeting {
    fn process(
        &mut self,
        invocation: &MacroInvocation<'_>,
        _: &mut MacroExecutionContext,
    ) -> Result<BlockMacroOutput, String> {
        Ok(BlockMacroOutput::Paragraph(format!(
            "Hello, {}!",
            invocation.target
        )))
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut registrations = Registry::builder();
    registrations.register(BlockMacro::new("greet", || Greeting))?;
    let mut available = ExtensionCatalog::default();
    available.register("greetings", registrations.build())?;

    // Availability alone does not activate a package or invoke its factories.
    let selected = available.select(["greetings"])?.ok_or("empty selection")?;
    let mut backends = ConverterRegistry::default();
    backends.register(REGISTRATION, |variant| {
        move |options, parser| Converter::new_with_variant(options, parser, variant)
    })?;
    let make_converter = backends.resolve("html", None)?;
    let pipeline = Pipeline::new(make_converter(
        Options::builder().embedded(true).build(),
        ParserOptions::builder(),
    )?)
    .with_extensions(selected);
    let parsed = pipeline.parse("ifdef::backend-html5[]\ngreet::World[]\nendif::[]")?;
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
