//! Generate expected HTML output files for integration tests.
//!
//! Usage:
//!   `cargo run -p acdc-converters-html --example generate_expected_fixtures`

use acdc_converters_common::generate_fixtures::FixtureGenerator;
use acdc_converters_common::{GeneratorMetadata, Options, Processable};
use acdc_converters_html::{Processor, RenderOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    FixtureGenerator::new("html", "html").generate(|doc, output| {
        let options = Options {
            generator_metadata: GeneratorMetadata::new("acdc", "0.1.0"),
            ..Default::default()
        };
        let processor = Processor::new(options, doc.attributes.clone());
        let render_options = RenderOptions::default();
        processor.convert_to_writer(doc, output, &render_options)?;
        Ok(())
    })
}
