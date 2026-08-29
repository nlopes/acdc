//! Shared fixture generation utilities for converter integration tests.
//!
//! This module provides a `FixtureGenerator` builder that abstracts common
//! fixture generation boilerplate across all converters.
//!
//! # Example
//!
//! ```ignore
//! use acdc_converters_dev::generate_fixtures::FixtureGenerator;
//!
//! // Simple: process top-level files and auto-discover subdirectories
//! FixtureGenerator::new("manpage", "man")
//!     .generate(|subdir, doc, output| {
//!         let embedded = subdir == Some("embedded");
//!         let options = Options::builder().embedded(embedded).build();
//!         let processor = Processor::new(options, doc.attributes.clone());
//!         processor.convert_to_writer(doc, output)?;
//!         Ok(())
//!     })?;
//!
//! // Variants: scope to each top-level subdirectory
//! let gen = FixtureGenerator::new("html", "html");
//! for variant in gen.subdirs()? {
//!     gen.in_subdir(&variant).generate(|mode, doc, output| {
//!         // variant = "html" or "html5s", mode = Some("embedded") or Some("standalone")
//!         Ok(())
//!     })?;
//! }
//! ```

use std::{collections::HashSet, error::Error, fs, path::Path, path::PathBuf};

use acdc_parser::{Document, DocumentAttributes, Options};
use crossterm::style::{PrintStyledContent, Stylize};

/// Builder for generating expected fixture output files.
///
/// Handles directory scanning, `AsciiDoc` parsing, error reporting, and file writing.
/// Each converter provides a closure to handle the actual conversion.
pub struct FixtureGenerator {
    converter_name: String,
    output_extension: String,
    subdir: Option<String>,
    fixture_names: Option<HashSet<String>>,
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
            subdir: None,
            fixture_names: None,
        }
    }

    /// Return a new generator scoped to a subdirectory.
    ///
    /// The returned generator operates on `tests/fixtures/source/{subdir}/`
    /// and `tests/fixtures/expected/{subdir}/` instead of the root fixture
    /// directories.
    #[must_use]
    pub fn in_subdir(&self, subdir: &str) -> Self {
        Self {
            converter_name: self.converter_name.clone(),
            output_extension: self.output_extension.clone(),
            subdir: Some(subdir.to_string()),
            fixture_names: self.fixture_names.clone(),
        }
    }

    /// Return a new generator limited to the listed source fixture stems.
    ///
    /// Names omit the `.adoc` extension. The filter applies to top-level and
    /// discovered subdirectory fixtures.
    #[must_use]
    pub fn with_fixtures(&self, fixture_names: &[&str]) -> Self {
        Self {
            converter_name: self.converter_name.clone(),
            output_extension: self.output_extension.clone(),
            subdir: self.subdir.clone(),
            fixture_names: Some(
                fixture_names
                    .iter()
                    .map(|name| (*name).to_string())
                    .collect(),
            ),
        }
    }

    /// Discover subdirectory names under the source fixture directory.
    ///
    /// Returns sorted directory names found directly inside the source
    /// directory (e.g., `["embedded"]` or `["html", "html5s"]`).
    ///
    /// # Errors
    ///
    /// Returns an error if the source directory cannot be read.
    pub fn subdirs(&self) -> Result<Vec<String>, Box<dyn Error>> {
        sorted_subdirs(&self.source_dir())
    }

    fn source_dir(&self) -> PathBuf {
        let mut path = PathBuf::from("converters")
            .join(&self.converter_name)
            .join("tests/fixtures/source");
        if let Some(ref subdir) = self.subdir {
            path = path.join(subdir);
        }
        path
    }

    fn expected_dir(&self) -> PathBuf {
        let mut path = PathBuf::from("converters")
            .join(&self.converter_name)
            .join("tests/fixtures/expected");
        if let Some(ref subdir) = self.subdir {
            path = path.join(subdir);
        }
        path
    }

    /// Generate fixture outputs using the provided conversion function.
    ///
    /// Scans the source directory for `.adoc` files and generates expected
    /// outputs in the corresponding expected directory. Also discovers and
    /// processes any subdirectories, passing the subdirectory name to the
    /// conversion function (`None` for top-level files).
    ///
    /// # Errors
    ///
    /// Returns an error if directory creation or file I/O fails.
    pub fn generate<F>(&self, convert_fn: F) -> Result<(), Box<dyn Error>>
    where
        F: Fn(Option<&str>, &Document, &mut Vec<u8>) -> Result<(), Box<dyn Error>>,
    {
        let base_source = self.source_dir();
        let base_expected = self.expected_dir();

        // Process top-level files
        self.generate_dir(&base_source, &base_expected, &|doc, output| {
            convert_fn(None, doc, output)
        })?;

        // Process subdirectories
        for subdir in sorted_subdirs(&base_source)? {
            let input_dir = base_source.join(&subdir);
            let output_dir = base_expected.join(&subdir);
            self.generate_dir(&input_dir, &output_dir, &|doc, output| {
                convert_fn(Some(&subdir), doc, output)
            })?;
        }

        Ok(())
    }

    fn generate_dir<F>(
        &self,
        input_dir: &Path,
        output_dir: &Path,
        convert_fn: &F,
    ) -> Result<(), Box<dyn Error>>
    where
        F: Fn(&Document, &mut Vec<u8>) -> Result<(), Box<dyn Error>>,
    {
        // Ensure output directory exists
        fs::create_dir_all(output_dir)?;

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
            .filter(|e| self.includes_fixture(&e.path()))
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
            let parser_options = Options::builder()
                .with_attributes(DocumentAttributes::default())
                .build();

            let parsed = match acdc_parser::parse_file(&input_path, &parser_options) {
                Ok(parsed) => parsed,
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
            if let Err(e) = convert_fn(parsed.document(), &mut output) {
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

    fn includes_fixture(&self, path: &Path) -> bool {
        self.fixture_names.as_ref().is_none_or(|fixture_names| {
            path.file_stem()
                .and_then(std::ffi::OsStr::to_str)
                .is_some_and(|name| fixture_names.contains(name))
        })
    }
}

/// Return sorted directory names found directly inside `dir`.
fn sorted_subdirs(dir: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let mut names: Vec<_> = dir
        .read_dir()?
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_filter_matches_only_selected_source_stems() {
        let generator = FixtureGenerator::new("terminal", "osc8.txt")
            .with_fixtures(&["comprehensive", "url_macro"]);

        assert!(generator.includes_fixture(Path::new("comprehensive.adoc")));
        assert!(generator.includes_fixture(Path::new("url_macro.adoc")));
        assert!(!generator.includes_fixture(Path::new("document.adoc")));
    }
}
