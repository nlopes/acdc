//! Generate expected Terminal output files for integration tests.
//!
//! Usage:
//!   `cargo run -p acdc-converters-terminal --features images,highlighting --example generate_expected_fixtures`

use acdc_converters_core::{Options, Processable};
use acdc_converters_dev::generate_fixtures::FixtureGenerator;
use acdc_converters_terminal::Processor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    FixtureGenerator::new("terminal", "txt").generate(|doc, output| {
        let processor = Processor::new(Options::default(), doc.attributes.clone());
        processor.convert_to_writer(doc, output)?;
        Ok(())
    })
}
