//! Generate expected Terminal output files for integration tests.
//!
//! Usage:
//!   `cargo run --all-features --example generate_terminal_fixtures`

use acdc_converters_core::{Converter, Options};
use acdc_converters_dev::generate_fixtures::FixtureGenerator;
use acdc_converters_terminal::Processor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    FixtureGenerator::new("terminal", "txt").generate(|doc, output| {
        let processor =
            Processor::new(Options::default(), doc.attributes.clone()).with_terminal_width(80);
        processor.write_to(doc, output, None)?;
        Ok(())
    })
}
