//! Generate expected Terminal output files for integration tests.
//!
//! Usage:
//!   `cargo run --all-features --example generate_terminal_fixtures`

use acdc_converters_core::{Converter, Options};
use acdc_converters_dev::generate_fixtures::FixtureGenerator;
use acdc_converters_terminal::{Capabilities, Processor};

macro_rules! terminal_fixture_catalog {
    ( [ $( ($name:ident, $has_osc8_variant:expr $(, requires: $cfg:meta )? ) ),* $(,)? ] ) => {
        const TERMINAL_FIXTURES: &[(&str, bool)] = &[
            $((stringify!($name), $has_osc8_variant)),*
        ];
    };
}

include!("../tests/fixtures/catalog.rs");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    crossterm::style::force_color_output(true);

    generate(&FixtureGenerator::new("terminal", "txt"), false)?;

    let osc8_fixtures = TERMINAL_FIXTURES
        .iter()
        .filter_map(|(name, has_osc8_variant)| has_osc8_variant.then_some(*name))
        .collect::<Vec<_>>();
    let osc8_generator =
        FixtureGenerator::new("terminal", "osc8.txt").with_fixtures(osc8_fixtures.as_slice());
    generate(&osc8_generator, true)
}

fn generate(
    generator: &FixtureGenerator,
    osc8_links: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    generator.generate(|_subdir, doc, output| {
        let processor = Processor::new(Options::default(), doc.attributes.clone())
            .with_terminal_width(80)
            .with_dark_mode(true)
            .with_terminal_capabilities(Capabilities {
                unicode: true,
                osc8_links,
            });
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        processor.write_to(doc, output, None, None, &mut diagnostics)?;
        Ok(())
    })
}
