//! Ordinary HTML rendering of the native library's generated nodes.

use std::{fs, path::Path};

use acdc_converters_html::HtmlVariant;
use acdc_parser::CompatibilityMode;

mod support;

#[test]
fn lists_render_through_the_shared_pipeline_in_both_modes_and_html_variants()
-> Result<(), Box<dyn std::error::Error>> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for name in ["default", "nested"] {
        let source = fs::read_to_string(fixtures.join(format!("{name}.adoc")))?;
        for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
            for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
                let output = support::render(&source, mode, variant)?;
                let expected =
                    fs::read_to_string(fixtures.join(format!("{name}.{}.html", variant.as_str())))?;
                assert_eq!(output, expected, "{name}: {mode:?}, {variant:?}");
            }
        }
    }
    Ok(())
}
