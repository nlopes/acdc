//! Generate expected Markdown output files for integration tests.

use std::{error::Error, fs, path::Path};

use acdc_converters_dev::fixtures::source_files;

#[path = "../tests/support/mod.rs"]
mod fixture_support;

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let requested_fixture = arguments.next();
    if arguments.next().is_some() {
        return Err("usage: generate_markdown_fixtures [fixture-name]".into());
    }

    let source_dir = Path::new("converters/markdown/tests/fixtures/source");
    let expected_dir = Path::new("converters/markdown/tests/fixtures/expected");
    let fixtures = source_files(source_dir)?
        .into_iter()
        .filter(|path| {
            requested_fixture
                .as_deref()
                .is_none_or(|requested| path.file_stem() == Some(requested))
        })
        .collect::<Vec<_>>();

    if fixtures.is_empty()
        && let Some(requested) = requested_fixture
    {
        return Err(format!("unknown Markdown fixture: {}", requested.to_string_lossy()).into());
    }

    for input_path in fixtures {
        let stem = input_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or("invalid fixture file name")?;
        let rendered = fixture_support::render(&input_path)?;
        let output_path = expected_dir.join(format!("{stem}.md"));
        fs::write(output_path, rendered.output)?;
    }

    Ok(())
}
