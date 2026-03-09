//! Generate expected HTML output files for integration tests.
//!
//! Discovers variant subdirectories (`html`, `html5s`) under `tests/fixtures/source/`
//! and generates expected outputs for each.
//!
//! Usage:
//!   `cargo run --example generate_html_fixtures`

use acdc_converters_core::{Backend, Converter, GeneratorMetadata, Options};
use acdc_converters_dev::generate_fixtures::FixtureGenerator;
use acdc_converters_html::{Processor, RenderOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let generator = FixtureGenerator::new("html", "html");
    for variant in generator.subdirs()? {
        generator
            .in_subdir(&variant)
            .generate(|mode, doc, output| {
                let backend = match variant.as_str() {
                    "html5s" => Backend::Html5s,
                    _ => Backend::Html,
                };
                let embedded = mode == Some("embedded");
                let options = Options::builder()
                    .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
                    .backend(backend)
                    .build();
                let processor = Processor::new(options, doc.attributes.clone());
                let render_options = RenderOptions {
                    embedded,
                    ..RenderOptions::default()
                };
                processor.convert_to_writer(doc, output, &render_options)?;
                Ok(())
            })?;
    }
    Ok(())
}
