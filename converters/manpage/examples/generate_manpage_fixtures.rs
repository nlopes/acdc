//! Generate expected Manpage output files for integration tests.
//!
//! Usage:
//!   `cargo run --example generate_manpage_fixtures`

use acdc_converters_core::{Converter, GeneratorMetadata, Options};
use acdc_converters_dev::generate_fixtures::FixtureGenerator;
use acdc_converters_manpage::Processor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    FixtureGenerator::new("manpage", "man").generate(|doc, output| {
        let options = Options::builder()
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .build();
        let processor = Processor::new(options, doc.attributes.clone());
        processor.write_to(doc, output, None)?;
        Ok(())
    })
}
