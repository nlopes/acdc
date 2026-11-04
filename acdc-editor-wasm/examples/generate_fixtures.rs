//! Generate selected editor source-highlighting and preview expectations.

use std::{error::Error, fs, path::Path};

use acdc_editor_wasm::parse_and_render_with_mode;
use acdc_parser::CompatibilityMode;

fn main() -> Result<(), Box<dyn Error>> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let names = std::env::args().skip(1).collect::<Vec<_>>();
    if names.is_empty() {
        return Err("select source_inlines or source_blocks".into());
    }
    for name in names {
        if !matches!(name.as_str(), "source_inlines" | "source_blocks") {
            return Err(format!("unknown fixture: {name}").into());
        }
        let source = fs::read_to_string(fixtures.join(format!("{name}.adoc")))?;
        for (label, mode) in [
            ("asciidoctor", CompatibilityMode::Asciidoctor),
            ("draft", CompatibilityMode::Draft),
        ] {
            let result = parse_and_render_with_mode(&source, mode)?;
            fs::write(
                fixtures.join(format!("{name}.{label}.highlight.html")),
                result.highlight_html,
            )?;
            fs::write(
                fixtures.join(format!("{name}.{label}.preview.html")),
                result.preview_html,
            )?;
        }
    }
    Ok(())
}
