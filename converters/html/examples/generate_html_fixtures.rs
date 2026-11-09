//! Generate HTML fixtures for the compiled feature configuration.
//!
//! With highlighting enabled, writes canonical `.html` expectations.
//! Without highlighting, writes only the required `.no-highlighting.html`
//! alternatives. Substitution fixtures follow the same highlighting rules.
//! Optional arguments select source fixture stems.
//!
//! Run from the workspace root.

use std::{error::Error, fs, path::Path};

use acdc_converters_dev::fixtures::{parse_fixture, source_files};
use acdc_converters_html::HtmlVariant;

#[path = "../tests/support/mod.rs"]
mod fixture_support;

fn main() -> Result<(), Box<dyn Error>> {
    let names = std::env::args().skip(1).collect::<Vec<_>>();
    let fixtures = Path::new("converters/html/tests/fixtures");
    for (directory, variant) in [
        ("html", HtmlVariant::Standard),
        ("html5s", HtmlVariant::Semantic),
    ] {
        for mode in ["embedded", "standalone"] {
            let source_dir = fixtures.join("source").join(directory).join(mode);
            let expected_dir = fixtures.join("expected").join(directory).join(mode);
            for path in source_files(&source_dir)? {
                let stem = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .ok_or("invalid HTML fixture stem")?;
                if !names.is_empty() && !names.iter().any(|name| name == stem) {
                    continue;
                }
                let parsed = parse_fixture(&path)?;
                let doc = parsed.document();
                if !cfg!(feature = "highlighting") && !fixture_support::has_highlighter(doc) {
                    continue;
                }
                let output_path = fixture_support::expected_fixture_path(&expected_dir, stem, doc);
                let rendered = fixture_support::render_document(doc, variant, mode == "embedded")?;
                fs::create_dir_all(&expected_dir)?;
                fs::write(&output_path, rendered.output)?;
                println!("Generated {}", output_path.display());
            }
        }
    }
    Ok(())
}
