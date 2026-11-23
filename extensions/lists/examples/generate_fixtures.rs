//! Generate only this package's new, explicitly selected HTML expectations.

use std::{fs, path::Path};

use acdc_converters_html::HtmlVariant;
use acdc_parser::CompatibilityMode;

#[path = "../tests/support/mod.rs"]
mod support;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for name in std::env::args().skip(1) {
        let source = fs::read_to_string(fixtures.join(format!("{name}.adoc")))?;
        for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
            let output = support::render(&source, CompatibilityMode::Asciidoctor, variant)?;
            let path = fixtures.join(format!("{name}.{}.html", variant.as_str()));
            fs::write(&path, output)?;
            println!("Generated {}", path.display());
        }
    }
    Ok(())
}
