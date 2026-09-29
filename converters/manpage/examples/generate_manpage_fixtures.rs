//! Generate expected Manpage output files for integration tests.
//!
//! Optional arguments select fixture stems; omit them to generate all fixtures.
//!
//! Usage:
//!   `cargo run --example generate_manpage_fixtures`

use acdc_converters_core::{Converter, GeneratorMetadata, Options};
use acdc_converters_dev::generate_fixtures::FixtureGenerator;
use acdc_converters_manpage::Processor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let names = arguments.iter().map(String::as_str).collect::<Vec<_>>();
    let generator = FixtureGenerator::new("manpage", "man");
    let generator = if names.is_empty() {
        generator
    } else {
        generator.with_fixtures(&names)
    };
    generator.generate(|subdir, doc, output| {
        let embedded = subdir == Some("embedded");
        let options = Options::builder()
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .embedded(embedded)
            .build();
        let processor = Processor::new(
            options,
            acdc_parser::Options::builder().with_attributes(doc.attributes.clone().into_inputs()),
        )?;
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("manpage");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        processor.write_document(doc, output, None, &mut diagnostics)?;
        Ok(())
    })
}
