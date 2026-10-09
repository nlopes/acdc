//! Generate HTML fixtures for the compiled feature configuration.
//!
//! With highlighting enabled, writes canonical `.html` expectations.
//! Without highlighting, writes only the required `.no-highlighting.html`
//! alternatives. Substitution-dependent fixtures are omitted when substitutions
//! are disabled. Optional arguments select source fixture stems.
//!
//! Run from the workspace root.

use std::{error::Error, fs, path::Path};

use acdc_converters_core::{Diagnostics, GeneratorMetadata, Options, WarningSource};
use acdc_converters_html::{HtmlVariant, Processor, RenderOptions};
use acdc_parser::{Options as ParserOptions, parse_file};

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
            let mut paths = source_dir
                .read_dir()?
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<Result<Vec<_>, _>>()?;
            paths.sort();
            for path in paths {
                if path.extension().is_none_or(|extension| extension != "adoc") {
                    continue;
                }
                let stem = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .ok_or("invalid HTML fixture stem")?;
                if fixture_support::skip_fixture(stem)
                    || (!names.is_empty() && !names.iter().any(|name| name == stem))
                {
                    continue;
                }
                let parsed = parse_file(
                    &path,
                    &ParserOptions::builder()
                        .with_safe_mode(acdc_parser::SafeMode::Unsafe)
                        .build()?,
                )?;
                let doc = parsed.document();
                if !cfg!(feature = "highlighting") && !fixture_support::has_highlighter(doc) {
                    continue;
                }
                let output_path = fixture_support::expected_fixture_path(&expected_dir, stem, doc);
                let options = Options::builder()
                    .safe_mode(acdc_parser::SafeMode::Unsafe)
                    .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
                    .build();
                let processor = Processor::new_with_variant(
                    options,
                    ParserOptions::builder()
                        .with_safe_mode(acdc_parser::SafeMode::Unsafe)
                        .with_attributes(doc.attributes.clone().into_inputs()),
                    variant,
                )?;
                let render_options = RenderOptions {
                    embedded: mode == "embedded",
                    ..RenderOptions::default()
                };
                let mut output = Vec::new();
                let mut warnings = Vec::new();
                let source = WarningSource::new("html").with_variant(variant.as_str());
                let mut diagnostics = Diagnostics::new(&source, &mut warnings);
                processor.convert_to_writer(doc, &mut output, &render_options, &mut diagnostics)?;
                fs::create_dir_all(&expected_dir)?;
                fs::write(&output_path, output)?;
                println!("Generated {}", output_path.display());
            }
        }
    }
    Ok(())
}
