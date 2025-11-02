//! Generate expected Terminal output files for integration tests.
//!
//! This tool generates `.txt` files from parser fixtures that can be used as
//! expected outputs in integration tests. It processes a list of fixture names
//! and generates Terminal output for each one.
//!
//! Usage:
//!   `cargo run --example generate_expected_terminal`
//!
//! This will generate Terminal output files in `tests/fixtures/expected/` for a curated
//! list of representative fixtures. After generation, manually review each file
//! to ensure quality before using it in tests.

use std::fs;
use std::path::PathBuf;

use acdc_converters_common::{Options, Processable};
use acdc_parser::{DocumentAttributes, Options as ParserOptions};
use acdc_terminal::Processor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Comprehensive list of fixtures that cover all structural elements
    let fixtures = vec![
        "document",
        "nested_sections",
        "ordered_list",
        "unordered_list",
        "description_list_mixed_content",
        "table_multi_cell_per_line",
        "delimited_block",
        "quote_block_with_paragraphs",
        "admonition_block",
        "footnotes",
        "url_macro",
        "basic_image_block",
    ];

    let input_dir = PathBuf::from("acdc-parser/fixtures/tests");
    let output_dir = PathBuf::from("converters/terminal/tests/fixtures/expected");

    // Ensure output directory exists
    fs::create_dir_all(&output_dir)?;

    println!("Generating expected Terminal outputs...\n");

    for fixture_name in fixtures {
        let input_path = input_dir.join(format!("{fixture_name}.adoc"));
        let output_path = output_dir.join(format!("{fixture_name}.txt"));

        // Skip if input doesn't exist
        if !input_path.exists() {
            println!("⚠️  Skipping {fixture_name}: input file not found");
            continue;
        }

        // Parse AsciiDoc
        let parser_options = ParserOptions {
            document_attributes: DocumentAttributes::default(),
            ..Default::default()
        };
        let doc = match acdc_parser::parse_file(&input_path, &parser_options) {
            Ok(doc) => doc,
            Err(e) => {
                println!("❌ Error parsing {fixture_name}: {e}");
                continue;
            }
        };

        // Convert to Terminal output
        let mut output = Vec::new();
        let processor = Processor::new(Options::default(), doc.attributes.clone());

        if let Err(e) = processor.convert_to_writer(&doc, &mut output) {
            println!("❌ Error converting {fixture_name}: {e}");
            continue;
        }

        // Write to file
        fs::write(&output_path, &output)?;

        println!("✓ Generated {fixture_name}.txt ({} bytes)", output.len());
    }

    println!(
        "\n✨ Done! Review the generated files in {}",
        output_dir.display()
    );
    println!("   Manually verify each file before using in tests.");

    Ok(())
}
