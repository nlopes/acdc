//! Regenerate JSON test fixtures from .adoc files
//!
//! Run with: `cargo run --example generate_parser_fixtures`

use std::path::PathBuf;

use crossterm::style::{PrintStyledContent, Stylize};

// List of .adoc files that are expected to produce parsing errors
const EXPECTED_ERRORING_ADOCS: &[&str] = &["section_with_invalid_subsection.adoc"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixtures_dir = PathBuf::from("acdc-parser/fixtures/tests");

    println!("Generating parser JSON fixtures...\n");

    let mut success_count = 0;
    let mut error_count = 0;

    for entry in fixtures_dir.read_dir()?.filter_map(Result::ok).filter(|e| {
        let file_name = e.file_name();
        !EXPECTED_ERRORING_ADOCS.contains(&file_name.to_str().unwrap_or("_THIS_WILL_NEVER_MATCH_"))
            && e.path().extension().is_some_and(|ext| ext == "adoc")
    }) {
        let path = entry.path();
        let json_path = path.with_extension("json");

        let options = acdc_parser::Options::default();
        match acdc_parser::parse_file(&path, &options) {
            Ok(doc) => {
                let json = serde_json::to_string_pretty(&doc)?;
                std::fs::write(&json_path, json)?;
                println!(
                    "{} Generated {}",
                    PrintStyledContent("✓".green()),
                    json_path.display()
                );
                success_count += 1;
            }
            Err(e) => {
                println!(
                    "{} Error parsing {}: {e}",
                    PrintStyledContent("❌".red()),
                    path.display()
                );
                error_count += 1;
            }
        }
    }

    println!();
    if error_count > 0 {
        println!("Completed with {error_count} error(s). {success_count} fixture(s) regenerated.");
    } else {
        println!(
            "✨ Done! {success_count} fixture(s) regenerated in {}\n⏭️ Skipped {} expected erroring .adoc files.",
            fixtures_dir.display(),
            EXPECTED_ERRORING_ADOCS.len()
        );
    }
    println!("   Manually verify each file before committing.");

    Ok(())
}
