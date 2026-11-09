//! Generate expected Terminal output files for integration tests.
//!
//! Optional arguments select fixture stems; omit them to generate all fixtures.
//! Without highlighting, output filenames include `.no-highlighting`.
//!
//! Usage:
//!   `cargo run --all-features --example generate_terminal_fixtures`

use acdc_converters_dev::generate_fixtures::FixtureGenerator;

#[path = "../tests/support/mod.rs"]
mod fixture_support;

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

    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let names = arguments.iter().map(String::as_str).collect::<Vec<_>>();
    let extension = if cfg!(feature = "highlighting") {
        "txt"
    } else {
        "no-highlighting.txt"
    };
    let generator = FixtureGenerator::new("terminal", extension);
    let generator = if names.is_empty() {
        generator
    } else {
        generator.with_fixtures(&names)
    };
    generate(&generator, false)?;

    let osc8_fixtures = TERMINAL_FIXTURES
        .iter()
        .filter_map(|(name, has_osc8_variant)| {
            (*has_osc8_variant && (names.is_empty() || names.contains(name))).then_some(*name)
        })
        .collect::<Vec<_>>();
    let extension = if cfg!(feature = "highlighting") {
        "osc8.txt"
    } else {
        "no-highlighting.osc8.txt"
    };
    let osc8_generator =
        FixtureGenerator::new("terminal", extension).with_fixtures(osc8_fixtures.as_slice());
    generate(&osc8_generator, true)
}

fn generate(
    generator: &FixtureGenerator,
    osc8_links: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    generator.generate(|_, path, output| {
        output.extend(fixture_support::render(path, osc8_links)?.output);
        Ok(())
    })
}
