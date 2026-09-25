//! Generate expected HTML output files for integration tests.
//!
//! Discovers variant subdirectories (`html`, `html5s`) under `tests/fixtures/source/`
//! and generates expected outputs for each.
//!
//! Optional arguments select fixture stems; omit them to generate all fixtures.
//!
//! Usage:
//!   `cargo run --example generate_html_fixtures`

use acdc_converters_core::{GeneratorMetadata, Options};
use acdc_converters_dev::generate_fixtures::FixtureGenerator;
use acdc_converters_html::{HtmlVariant, Processor, RenderOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let names = arguments.iter().map(String::as_str).collect::<Vec<_>>();
    let generator = FixtureGenerator::new("html", "html");
    let generator = if names.is_empty() {
        generator
    } else {
        generator.with_fixtures(&names)
    };
    for variant in generator.subdirs()? {
        generator
            .in_subdir(&variant)
            .generate(|mode, doc, output| {
                let html_variant = match variant.as_str() {
                    "html5s" => HtmlVariant::Semantic,
                    _ => HtmlVariant::Standard,
                };
                let embedded = mode == Some("embedded");
                let options = Options::builder()
                    .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
                    .build();
                let processor = Processor::new_with_variant(
                    options,
                    acdc_parser::Options::builder()
                        .with_attributes(doc.attributes.clone().into_inputs()),
                    html_variant,
                )?;
                let render_options = RenderOptions {
                    embedded,
                    ..RenderOptions::default()
                };
                let mut warnings = Vec::new();
                let source = acdc_converters_core::WarningSource::new("html");
                let mut diagnostics =
                    acdc_converters_core::Diagnostics::new(&source, &mut warnings);
                processor.convert_to_writer(doc, output, &render_options, &mut diagnostics)?;
                Ok(())
            })?;
    }
    Ok(())
}
