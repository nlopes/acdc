//! Generate expected Manpage output files for integration tests.
//!
//! Usage:
//!   `cargo run -p acdc-converters-manpage --example generate_expected_fixtures`

use acdc_converters_common::generate_fixtures::FixtureGenerator;
use acdc_converters_common::{GeneratorMetadata, Options, Processable};
use acdc_converters_manpage::Processor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    FixtureGenerator::new("manpage", "man").generate(|doc, output| {
        let options = Options {
            generator_metadata: GeneratorMetadata::new("acdc", "0.1.0"),
            ..Default::default()
        };
        let processor = Processor::new(options, doc.attributes.clone());
        processor.convert_to_writer(doc, output)?;
        Ok(())
    })
}
