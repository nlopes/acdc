//! Shared fixture generation utilities for converter integration tests.
//!
//! This module provides a `FixtureGenerator` builder that abstracts common
//! fixture generation boilerplate across all converters.
//!
//! # Example
//!
//! ```ignore
//! use acdc_converters_common::generate_fixtures::FixtureGenerator;
//!
//! FixtureGenerator::new("manpage", "man")
//!     .generate(|doc, output| {
//!         let processor = Processor::new(Options::default(), doc.attributes.clone());
//!         processor.convert_to_writer(doc, output)?;
//!         Ok(())
//!     })?;
//! ```

use std::path::PathBuf;
use std::{error::Error, fs};

use acdc_parser::{Document, Options as ParserOptions};
use crossterm::style::{PrintStyledContent, Stylize};

/// Builder for generating expected fixture output files.
///
/// Handles directory scanning, `AsciiDoc` parsing, error reporting, and file writing.
/// Each converter provides a closure to handle the actual conversion.
pub struct FixtureGenerator {
    converter_name: String,
    output_extension: String,
}

impl FixtureGenerator {
    /// Create a new fixture generator for a converter.
    ///
    /// # Arguments
    ///
    /// * `converter_name` - Name of the converter (e.g., "manpage", "html", "terminal")
    /// * `output_extension` - File extension for output files (e.g., "man", "html", "txt")
    #[must_use]
    pub fn new(converter_name: &str, output_extension: &str) -> Self {
        Self {
            converter_name: converter_name.to_string(),
            output_extension: output_extension.to_string(),
        }
    }

    /// Generate fixture outputs using the provided conversion function.
    ///
    /// The conversion function receives a parsed document and a mutable output buffer.
    /// It should write the converted output to the buffer.
    ///
    /// # Errors
    ///
    /// Returns an error if directory creation or file I/O fails.
    pub fn generate<F>(&self, convert_fn: F) -> Result<(), Box<dyn Error>>
    where
        F: Fn(&Document, &mut Vec<u8>) -> Result<(), Box<dyn Error>>,
    {
        let input_dir = PathBuf::from("converters")
            .join(&self.converter_name)
            .join("tests/fixtures/source");

        let output_dir = PathBuf::from("converters")
            .join(&self.converter_name)
            .join("tests/fixtures/expected");

        // Ensure output directory exists
        fs::create_dir_all(&output_dir)?;

        println!(
            "Generating expected {} outputs...\n",
            self.converter_name.to_uppercase()
        );

        let mut success_count = 0;
        let mut error_count = 0;

        for entry in input_dir
            .read_dir()?
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "adoc"))
        {
            let input_path = entry.path();
            let Some(output_path) = input_path
                .file_stem()
                .map(|name| output_dir.join(name).with_extension(&self.output_extension))
            else {
                eprintln!(
                    "{} Skipping {}: unable to determine output file name",
                    PrintStyledContent("?".yellow()),
                    input_path.display()
                );
                continue;
            };

            // Parse AsciiDoc with rendering defaults
            let parser_options = ParserOptions {
                document_attributes: crate::default_rendering_attributes(),
                ..Default::default()
            };

            let doc = match acdc_parser::parse_file(&input_path, &parser_options) {
                Ok(doc) => doc,
                Err(e) => {
                    println!(
                        "{} Error parsing {}: {e}",
                        PrintStyledContent("❌".red()),
                        input_path.display()
                    );
                    error_count += 1;
                    continue;
                }
            };

            // Convert using the provided function
            let mut output = Vec::new();
            if let Err(e) = convert_fn(&doc, &mut output) {
                println!(
                    "{} Error converting {} to {}: {e}",
                    PrintStyledContent("❌".red()),
                    input_path.display(),
                    output_path.display()
                );
                error_count += 1;
                continue;
            }

            // Write to file
            fs::write(&output_path, &output)?;
            success_count += 1;

            println!(
                "{} Generated {} ({} bytes)",
                PrintStyledContent("✓".green()),
                output_path.display(),
                output.len()
            );
        }

        println!();
        if error_count > 0 {
            println!(
                "⚠️  Completed with {error_count} error(s). {success_count} file(s) generated."
            );
        } else {
            println!(
                "✨ Done! {success_count} file(s) generated in {}",
                output_dir.display()
            );
        }
        println!("   Manually verify each file before using in tests.");

        Ok(())
    }
}
