//! Generate expected HTML output files for integration tests.
//!
//! This tool generates `.html` files from parser fixtures that can be used as
//! expected outputs in integration tests. It processes a list of fixture names
//! and generates HTML for each one.
//!
//! Usage:
//!   `cargo run --example generate_expected_html`
//!
//! This will generate HTML files in `tests/fixtures/expected/` for a curated
//! list of representative fixtures. After generation, manually review each file
//! to ensure quality before using it in tests.

use std::fs;
use std::path::PathBuf;

use acdc_converters_common::{Options, Processable};
use acdc_converters_html::{Processor, RenderOptions};
use acdc_parser::Options as ParserOptions;
use crossterm::style::{PrintStyledContent, Stylize};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input_dir = PathBuf::from("converters/html/tests/fixtures/source");
    let output_dir = PathBuf::from("converters/html/tests/fixtures/expected");

    // Ensure output directory exists
    fs::create_dir_all(&output_dir)?;

    println!("Generating expected HTML outputs...\n");

    for fixture_filename in input_dir
        .read_dir()?
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "adoc"))
        .map(|e| e.path())
    {
        let input_path = &fixture_filename;
        let output_path = if let Some(file_path) = fixture_filename
            .file_stem()
            .map(|name| output_dir.join(name))
        {
            file_path.with_extension("html")
        } else {
            eprintln!(
                "{} Skipping {}: unable to determine output file name",
                PrintStyledContent("?".yellow()),
                input_path.display()
            );
            continue;
        };

        // Skip if input doesn't exist
        if !input_path.exists() {
            eprintln!(
                "{} Skipping generating {}: input file ({}) not found",
                PrintStyledContent("?".yellow()),
                output_path.display(),
                input_path.display()
            );
            continue;
        }

        // Parse AsciiDoc with rendering defaults
        let parser_options = ParserOptions {
            document_attributes: acdc_converters_common::default_rendering_attributes(),
            ..Default::default()
        };
        let doc = match acdc_parser::parse_file(input_path, &parser_options) {
            Ok(doc) => doc,
            Err(e) => {
                println!(
                    "{} Error parsing {}: {e}",
                    PrintStyledContent("❌".red()),
                    input_path.display()
                );
                continue;
            }
        };

        // Convert to HTML
        let mut output = Vec::new();
        let converter_options = Options {
            generator_metadata: acdc_converters_common::GeneratorMetadata::new("acdc", "0.1.0"),
            ..Default::default()
        };
        let processor = Processor::new(converter_options, doc.attributes.clone());
        let render_options = RenderOptions::default();

        if let Err(e) = processor.convert_to_writer(&doc, &mut output, &render_options) {
            println!(
                "{} Error converting {} to {}: {e}",
                PrintStyledContent("❌".red()),
                input_path.display(),
                output_path.display()
            );
            continue;
        }

        // Write to file
        fs::write(&output_path, &output)?;

        println!(
            "{} Generated {} ({} bytes)",
            PrintStyledContent("✓".green()),
            output_path.display(),
            output.len()
        );
    }

    println!(
        "\n✨ Done! Review the generated files in {}",
        output_dir.display()
    );
    println!("   Manually verify each file before using in tests.");

    Ok(())
}
