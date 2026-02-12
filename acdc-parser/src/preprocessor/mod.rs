//! The preprocessor module is responsible for processing the input document and expanding include directives.
use std::fmt::Write as _;
use std::path::Path;

use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE};

use crate::{
    Options,
    error::{Error, Positioning, SourceLocation},
    model::{LeveloffsetRange, Position, SourceRange},
};

mod attribute;
mod conditional;
mod include;
mod tag;

use include::{Include, IncludeResult};

/// Result from preprocessing that includes both the processed text and metadata needed
/// for accurate parsing (like leveloffset ranges).
#[derive(Debug, Default)]
pub(crate) struct PreprocessorResult {
    /// The preprocessed document text.
    pub(crate) text: String,
    /// Byte ranges where specific leveloffset values apply.
    /// Used by the parser to adjust section levels.
    pub(crate) leveloffset_ranges: Vec<LeveloffsetRange>,
    /// Byte ranges mapping preprocessed output back to source files.
    /// Used by the parser to produce accurate file/line info in warnings.
    pub(crate) source_ranges: Vec<SourceRange>,
}

/// Mutable state accumulated during preprocessing.
struct PreprocessorState {
    lines: Vec<String>,
    byte_offset: usize,
    leveloffset_ranges: Vec<LeveloffsetRange>,
    source_ranges: Vec<SourceRange>,
}

impl PreprocessorState {
    fn push_line(&mut self, line: String) {
        self.byte_offset += line.len() + 1;
        self.lines.push(line);
    }
}

/// BOM (Byte Order Mark) patterns for encoding detection
const BOM_PATTERNS: &[(&[u8], &Encoding, usize, &str)] = &[
    (&[0xEF, 0xBB, 0xBF], UTF_8, 3, "UTF-8"),
    (&[0xFF, 0xFE], UTF_16LE, 2, "UTF-16 LE"),
    (&[0xFE, 0xFF], UTF_16BE, 2, "UTF-16 BE"),
];

/// Reads a file and decodes it based on BOM (Byte Order Mark) or explicit encoding.
///
/// Supports:
/// - UTF-8 with BOM (EF BB BF)
/// - UTF-16 LE with BOM (FF FE)
/// - UTF-16 BE with BOM (FE FF)
/// - UTF-8 without BOM (fallback)
/// - Explicit encoding via `encoding` parameter
///
/// # Errors
/// Returns an error if:
/// - The file cannot be read
/// - The explicit encoding label is unknown
/// - The file is not valid UTF-8 and has no BOM
pub(crate) fn read_and_decode_file(
    file_path: &Path,
    encoding: Option<&str>,
) -> Result<String, Error> {
    let bytes = std::fs::read(file_path)?;

    // If there was an encoding specified, decode the entire file as that
    if let Some(enc_label) = encoding {
        if let Some(encoding) = Encoding::for_label(enc_label.as_bytes()) {
            let (cow, _, had_errors) = encoding.decode(&bytes);
            if had_errors {
                tracing::error!(
                    path = ?file_path.display(),
                    encoding = %enc_label,
                    "decoding encountered errors"
                );
            }
            return Ok(cow.into_owned());
        }
        return Err(Error::UnknownEncoding(enc_label.to_string()));
    }

    // Check for BOM patterns and decode accordingly
    for (bom, encoding, skip, name) in BOM_PATTERNS {
        if bytes.starts_with(bom)
            && let Some(content) = bytes.get(*skip..)
        {
            let (cow, _, had_errors) = encoding.decode(content);
            if had_errors {
                tracing::error!(
                    path = ?file_path.display(),
                    encoding = name,
                    "decoding encountered errors"
                );
            }
            return Ok(cow.into_owned());
        }
    }

    // If no BOM, try decoding as UTF-8 directly
    let (cow, _, had_errors) = UTF_8.decode(&bytes);
    if !had_errors {
        return Ok(cow.into_owned());
    }

    // If you get here, the file is not valid UTF-8 (and no BOM)
    Err(Error::UnrecognizedEncodingInFile(
        file_path.display().to_string(),
    ))
}

#[derive(Debug, Default)]
pub(crate) struct Preprocessor;

impl Preprocessor {
    /// Helper to create a `SourceLocation` from preprocessor context (line-level precision).
    ///
    /// Since the preprocessor operates line-by-line and doesn't track column positions,
    /// we use column=1 as a placeholder. The line number and offset still provide
    /// useful location information for error messages.
    fn create_source_location(line_number: usize, file_parent: Option<&Path>) -> SourceLocation {
        SourceLocation {
            file: file_parent.map(Path::to_path_buf),
            positioning: Positioning::Position(Position {
                line: line_number,
                column: 0, // Preprocessor doesn't track column - use 0 as placeholder
            }),
        }
    }

    fn normalize(input: &str) -> String {
        // Pre-allocate string with input length as estimate
        // (trimming end may reduce size slightly, but close enough)
        let lines: Vec<&str> = input.lines().map(str::trim_end).collect();
        let mut result = String::with_capacity(input.len());
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                result.push('\n');
            }
            result.push_str(line);
        }
        result
    }

    #[tracing::instrument(skip(reader))]
    pub(crate) fn process_reader<R: std::io::Read>(
        &self,
        mut reader: R,
        options: &Options,
    ) -> Result<PreprocessorResult, Error> {
        let mut input = String::new();
        reader.read_to_string(&mut input).map_err(|e| {
            tracing::error!(error=?e, "failed to read from reader");
            e
        })?;
        self.process(&input, options)
    }

    #[tracing::instrument]
    pub(crate) fn process(
        &self,
        input: &str,
        options: &Options,
    ) -> Result<PreprocessorResult, Error> {
        self.process_inner(input, None, options)
    }

    #[tracing::instrument(skip(file_path))]
    pub(crate) fn process_file<P: AsRef<Path>>(
        &self,
        file_path: P,
        options: &Options,
    ) -> Result<PreprocessorResult, Error> {
        if file_path.as_ref().parent().is_some() {
            // Use read_and_decode_file to support UTF-8, UTF-16 LE, and UTF-16 BE with BOM
            let input = read_and_decode_file(file_path.as_ref(), None)?;
            self.process_inner(&input, Some(file_path.as_ref()), options)
        } else {
            Err(Error::InvalidIncludePath(
                Box::new(Self::create_source_location(1, Some(file_path.as_ref()))),
                file_path.as_ref().to_path_buf(),
            ))
        }
    }

    /// Process an include directive.
    ///
    /// Returns the included content along with any leveloffset that applies.
    #[tracing::instrument]
    fn process_include(
        line: &str,
        line_number: usize,
        current_offset: usize,
        file_parent: Option<&Path>,
        options: &Options,
    ) -> Result<Option<IncludeResult>, Error> {
        if let Some(current_file_path) = file_parent {
            if let Some(parent_dir) = current_file_path.parent() {
                let include = Include::parse(
                    parent_dir,
                    line,
                    line_number,
                    current_offset,
                    Some(current_file_path),
                    options,
                )?;
                return Ok(Some(include.lines()?));
            }
        } else {
            tracing::error!(%line, "file parent is missing - include directive cannot be processed");
        }
        Ok(None)
    }

    /// Process a conditional directive (ifdef/ifndef/ifeval)
    #[tracing::instrument(skip(lines, attributes))]
    fn process_conditional<'a, I: Iterator<Item = &'a str>>(
        line: &str,
        lines: &mut std::iter::Peekable<I>,
        line_number: &mut usize,
        condition_line_number: usize,
        current_offset: usize,
        file_parent: Option<&Path>,
        attributes: &crate::DocumentAttributes,
    ) -> Result<Option<String>, Error> {
        let mut content = String::new();
        let condition =
            conditional::parse_line(line, condition_line_number, current_offset, file_parent)?;

        while let Some(next_line) = lines.peek() {
            if next_line.is_empty() {
                tracing::trace!(?line, "single line if directive");
                break;
            } else if next_line.starts_with("endif") {
                // Calculate the line number and offset for the endif line
                let endif_line_number = *line_number + 1;
                let endif_offset =
                    current_offset + line.len() + content.len() + content.lines().count();
                let endif = conditional::parse_endif(
                    next_line,
                    endif_line_number,
                    endif_offset,
                    file_parent,
                )?;

                if !endif.closes(&condition) {
                    tracing::warn!("attribute mismatch between if and endif directives");
                    return Err(Error::InvalidConditionalDirective(Box::new(
                        Self::create_source_location(endif_line_number, file_parent),
                    )));
                }
                tracing::trace!(?content, "multiline if directive");
                lines.next();
                *line_number += 1;
                break;
            }
            let _ = writeln!(content, "{next_line}");
            lines.next();
            *line_number += 1;
        }

        if condition.is_true(
            attributes,
            &mut content,
            condition_line_number,
            current_offset,
            file_parent,
        )? {
            Ok(Some(content))
        } else {
            Ok(None)
        }
    }

    #[tracing::instrument(skip(lines, attribute_content))]
    fn process_continuation<'a, I: Iterator<Item = &'a str>>(
        attribute_content: &mut String,
        lines: &mut std::iter::Peekable<I>,
        line_number: &mut usize,
    ) {
        while let Some(next_line) = lines.peek() {
            let next_line = next_line.trim();
            // If the next line isn't the end of a continuation, or a
            // continuation, we need to break out.
            if next_line.starts_with(':') || next_line.is_empty() {
                break;
            }
            // If we get here, and we get a hard wrap, keep everything as is.
            // If we get here, and we get a soft wrap, then remove the newline.
            // Anything else means we're at the end of the wrapped attribute, so
            // feed it and break.
            if next_line.ends_with(" + \\") {
                attribute_content.push_str(next_line);
                attribute_content.push('\n');
                lines.next();
                *line_number += 1;
            } else if next_line.ends_with(" \\") {
                attribute_content.push_str(next_line.trim_end_matches('\\'));
                lines.next();
                *line_number += 1;
            } else {
                attribute_content.push_str(next_line);
                lines.next();
                *line_number += 1;
                break;
            }
        }
    }

    /// Check if a line is a verbatim or raw block delimiter.
    ///
    /// Verbatim/raw blocks preserve content literally, including comments.
    /// Recognized delimiters:
    /// - `----` (listing/source blocks) - 4+ hyphens
    /// - `....` (literal blocks) - 4+ periods
    /// - `++++` (passthrough blocks) - 4+ plus signs
    /// - ` ``` ` (markdown code fences) - 3+ backticks
    #[tracing::instrument]
    fn is_verbatim_delimiter(line: &str) -> Option<&str> {
        let trimmed = line.trim();

        // Check for markdown code fences (3+ backticks)
        if trimmed.starts_with("```") {
            return Some("```");
        }

        // Check for other delimiters (4+ chars)
        //
        // We need to fetch the same delimiter size to make sure we close the block
        // correctly, and the minimum size is 4.
        let mut chars = trimmed.chars();
        let first_char = chars.next()?;
        if first_char != '-' && first_char != '.' && first_char != '+' {
            return None;
        }
        let mut idx = 1;
        for next_char in chars {
            if next_char == first_char {
                idx += 1;
            } else {
                break;
            }
        }
        if idx >= 4 {
            return trimmed.get(..idx);
        }
        None
    }

    /// Handle the result of processing an include directive.
    /// Records leveloffset and source ranges, and extends output with included lines.
    ///
    /// This also merges nested ranges from included files, adjusting their byte offsets
    /// to be relative to the current output position. This enables proper accumulation
    /// through arbitrarily deep include nesting.
    fn handle_include_result(include_result: IncludeResult, state: &mut PreprocessorState) {
        let start_offset = state.byte_offset;

        // Calculate the byte length of the included content
        let content_len: usize = include_result
            .lines
            .iter()
            .map(|l| l.len() + 1) // +1 for newline
            .sum();

        // If there's an effective leveloffset, record the range
        if let Some(leveloffset) = include_result.effective_leveloffset {
            if leveloffset != 0 {
                state.leveloffset_ranges.push(LeveloffsetRange::new(
                    start_offset,
                    start_offset + content_len,
                    leveloffset,
                ));
                tracing::trace!(
                    leveloffset,
                    start_offset,
                    end_offset = start_offset + content_len,
                    "Recording leveloffset range for include"
                );
            }
        }

        // Merge nested leveloffset ranges from the included file.
        // Shift their byte offsets to be relative to the current output position.
        // This enables proper accumulation through nested includes (A→B→C).
        for nested_range in include_result.nested_leveloffset_ranges {
            let adjusted_range = LeveloffsetRange::new(
                nested_range.start_offset + start_offset,
                nested_range.end_offset + start_offset,
                nested_range.value,
            );
            tracing::trace!(
                original_start = nested_range.start_offset,
                original_end = nested_range.end_offset,
                adjusted_start = adjusted_range.start_offset,
                adjusted_end = adjusted_range.end_offset,
                leveloffset = adjusted_range.value,
                "Merging nested leveloffset range"
            );
            state.leveloffset_ranges.push(adjusted_range);
        }

        // Record a SourceRange for the included content
        if let Some(file) = include_result.file {
            state.source_ranges.push(SourceRange {
                start_offset,
                end_offset: start_offset + content_len,
                file: file.clone(),
                start_line: 1,
            });
            tracing::trace!(
                ?file,
                start_offset,
                end_offset = start_offset + content_len,
                "Recording source range for include"
            );

            // Merge nested source ranges, adjusting byte offsets
            for nested_range in include_result.nested_source_ranges {
                state.source_ranges.push(SourceRange {
                    start_offset: nested_range.start_offset + start_offset,
                    end_offset: nested_range.end_offset + start_offset,
                    file: nested_range.file,
                    start_line: nested_range.start_line,
                });
            }
        }

        state.byte_offset += content_len;
        state.lines.extend(include_result.lines);
    }

    fn process_directive_line<'a>(
        line: &'a str,
        lines: &mut std::iter::Peekable<std::str::Lines<'a>>,
        line_number: &mut usize,
        current_offset: usize,
        file_parent: Option<&Path>,
        options: &Options,
        out: &mut PreprocessorState,
    ) -> Result<(), Error> {
        if line.starts_with("\\include")
            || line.starts_with("\\ifdef")
            || line.starts_with("\\ifndef")
            || line.starts_with("\\ifeval")
        {
            out.push_line(line[1..].to_string());
        } else if line.starts_with("ifdef")
            || line.starts_with("ifndef")
            || line.starts_with("ifeval")
        {
            let current_line = *line_number;
            if let Some(content) = Self::process_conditional(
                line,
                lines,
                line_number,
                current_line,
                current_offset,
                file_parent,
                &options.document_attributes,
            )? {
                out.push_line(content);
            }
        } else if line.starts_with("include") {
            if let Some(include_result) =
                Self::process_include(line, *line_number, current_offset, file_parent, options)?
            {
                Self::handle_include_result(include_result, out);
            }
        } else {
            out.push_line(line.to_string());
        }
        Ok(())
    }

    #[tracing::instrument]
    fn process_inner(
        &self,
        input: &str,
        file_parent: Option<&Path>,
        options: &Options,
    ) -> Result<PreprocessorResult, Error> {
        let input = Preprocessor::normalize(input);
        let mut options = options.clone();
        let output = Vec::with_capacity(input.lines().count());
        let mut lines = input.lines().peekable();
        let mut line_number = 1;
        let mut current_offset = 0;
        let mut out = PreprocessorState {
            lines: output,
            byte_offset: 0,
            leveloffset_ranges: Vec::new(),
            source_ranges: Vec::new(),
        };
        let mut in_verbatim_block = false;
        let mut current_delimiter: Option<&str> = None;

        while let Some(line) = lines.next() {
            if line.starts_with(':') && (line.ends_with(" + \\") || line.ends_with(" \\")) {
                let mut attribute_content = String::with_capacity(line.len() * 2);
                if line.ends_with(" + \\") {
                    attribute_content.push_str(line);
                    attribute_content.push('\n');
                } else if line.ends_with(" \\") {
                    attribute_content.push_str(line.trim_end_matches('\\'));
                }
                Self::process_continuation(&mut attribute_content, &mut lines, &mut line_number);
                attribute::parse_line(&mut options.document_attributes, attribute_content.as_str());
                out.push_line(attribute_content);
                continue;
            } else if line.starts_with(':') {
                attribute::parse_line(&mut options.document_attributes, line.trim());
            }
            if let Some(delimiter_type) = Self::is_verbatim_delimiter(line) {
                if in_verbatim_block && Some(delimiter_type) == current_delimiter {
                    tracing::trace!(?delimiter_type, "Closing verbatim block");
                    in_verbatim_block = false;
                    current_delimiter = None;
                } else if !in_verbatim_block {
                    tracing::trace!(?delimiter_type, "Opening verbatim block");
                    in_verbatim_block = true;
                    current_delimiter = Some(delimiter_type);
                }
                out.push_line(line.to_string());
            } else if line.starts_with("//") {
                out.push_line(line.to_string());
            } else if line.ends_with(']') && !line.starts_with('[') && line.contains("::") {
                Self::process_directive_line(
                    line,
                    &mut lines,
                    &mut line_number,
                    current_offset,
                    file_parent,
                    &options,
                    &mut out,
                )?;
            } else {
                out.push_line(line.to_string());
            }
            current_offset += line.len() + 1;
            line_number += 1;
        }

        Ok(PreprocessorResult {
            text: out.lines.join("\n"),
            leveloffset_ranges: out.leveloffset_ranges,
            source_ranges: out.source_ranges,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process() -> Result<(), Error> {
        let options = Options::default();
        let input = ":attribute: value

ifdef::attribute[]
content
endif::[]
";
        let result = Preprocessor.process(input, &options)?;
        assert_eq!(result.text, ":attribute: value\n\ncontent\n");
        Ok(())
    }

    #[test]
    fn test_good_endif_directive() -> Result<(), Error> {
        let options = Options::default();
        let input = ":asdf:

ifdef::asdf[]
content
endif::asdf[]";
        let result = Preprocessor.process(input, &options)?;
        assert_eq!(result.text, ":asdf:\n\ncontent\n");
        Ok(())
    }

    #[test]
    fn test_bad_endif_directive() {
        let options = Options::default();
        let input = "ifdef::asdf[]
content
endif::another[]";
        let output = Preprocessor.process(input, &options);
        assert!(matches!(
            output,
            Err(Error::InvalidConditionalDirective(..))
        ));
    }

    #[test]
    fn test_utf8_bom_detection() -> Result<(), Error> {
        let path = Path::new("fixtures/preprocessor/utf8_bom.adoc");
        let content = read_and_decode_file(path, None)?;

        // Should contain the test content without BOM
        assert!(content.contains("= Test Document"));
        assert!(content.contains("This is a test with special chars: é, ñ, ü."));
        // BOM should be stripped
        assert!(!content.starts_with('\u{FEFF}'));
        Ok(())
    }

    #[test]
    fn test_utf16le_bom_detection() -> Result<(), Error> {
        let path = Path::new("fixtures/preprocessor/utf16le_bom.adoc");
        let content = read_and_decode_file(path, None)?;

        // Should correctly decode UTF-16 LE content
        assert!(content.contains("= Test Document"));
        assert!(content.contains("This is a test with special chars: é, ñ, ü."));
        Ok(())
    }

    #[test]
    fn test_utf16be_bom_detection() -> Result<(), Error> {
        let path = Path::new("fixtures/preprocessor/utf16be_bom.adoc");
        let content = read_and_decode_file(path, None)?;

        // Should correctly decode UTF-16 BE content
        assert!(content.contains("= Test Document"));
        assert!(content.contains("This is a test with special chars: é, ñ, ü."));
        Ok(())
    }

    #[test]
    fn test_utf8_no_bom() -> Result<(), Error> {
        let path = Path::new("fixtures/preprocessor/utf8_no_bom.adoc");
        let content = read_and_decode_file(path, None)?;

        // Should decode regular UTF-8 file
        assert!(content.contains("= Test Document"));
        assert!(content.contains("This is a test with special chars: é, ñ, ü."));
        Ok(())
    }

    #[test]
    fn test_explicit_encoding_override() -> Result<(), Error> {
        // Test that explicit encoding parameter works
        let path = Path::new("fixtures/preprocessor/utf8_no_bom.adoc");
        let content = read_and_decode_file(path, Some("utf-8"))?;

        assert!(content.contains("= Test Document"));
        Ok(())
    }

    #[test]
    fn test_unknown_encoding_error() {
        let path = Path::new("fixtures/preprocessor/utf8_no_bom.adoc");
        let result = read_and_decode_file(path, Some("unknown-encoding-12345"));

        assert!(matches!(result, Err(Error::UnknownEncoding(_))));
    }

    #[test]
    fn test_include_utf16_file() -> Result<(), Error> {
        // Test that include directive works with UTF-16 LE files
        let preprocessor = Preprocessor;
        let path = Path::new("fixtures/preprocessor/main_with_include.adoc");
        let options = Options::default();

        let result = preprocessor.process_file(path, &options)?;

        // Should contain content from both main file and included UTF-16 file
        assert!(result.text.contains("= Main Document"));
        assert!(result.text.contains("This is included content."));
        assert!(result.text.contains("With special characters: é, ñ, ü."));
        assert!(result.text.contains("After include."));
        Ok(())
    }

    // === Tag Filtering Integration Tests ===

    #[test]
    fn test_include_with_single_tag() -> Result<(), Error> {
        let preprocessor = Preprocessor;
        let path = Path::new("fixtures/preprocessor/include_with_tag.adoc");
        let options = Options::default();

        let result = preprocessor.process_file(path, &options)?;

        // Should contain the intro tag content
        assert!(result.text.contains("This is the introduction."));
        assert!(result.text.contains("It has multiple lines."));
        // Should NOT contain other content
        assert!(!result.text.contains("untagged content"));
        assert!(!result.text.contains("main content"));
        assert!(!result.text.contains("Debug information"));
        // Should NOT contain tag directives
        assert!(!result.text.contains("tag::intro"));
        assert!(!result.text.contains("end::intro"));
        Ok(())
    }

    #[test]
    fn test_include_with_multiple_tags() -> Result<(), Error> {
        let preprocessor = Preprocessor;
        let path = Path::new("fixtures/preprocessor/include_multiple_tags.adoc");
        let options = Options::default();

        let result = preprocessor.process_file(path, &options)?;

        // Should contain both intro and main content
        assert!(result.text.contains("This is the introduction."));
        assert!(result.text.contains("This is the main content."));
        // Should NOT contain debug or untagged content
        assert!(!result.text.contains("Debug information"));
        Ok(())
    }

    #[test]
    fn test_include_with_wildcard_excluding_tag() -> Result<(), Error> {
        let preprocessor = Preprocessor;
        let path = Path::new("fixtures/preprocessor/include_wildcard_exclude.adoc");
        let options = Options::default();

        let result = preprocessor.process_file(path, &options)?;

        // Should contain intro and main content
        assert!(result.text.contains("This is the introduction."));
        assert!(result.text.contains("This is the main content."));
        // Should NOT contain debug content
        assert!(!result.text.contains("Debug information"));
        Ok(())
    }

    #[test]
    fn test_include_with_double_wildcard() -> Result<(), Error> {
        let preprocessor = Preprocessor;
        let path = Path::new("fixtures/preprocessor/include_double_wildcard.adoc");
        let options = Options::default();

        let result = preprocessor.process_file(path, &options)?;

        // Should contain all content except tag directive lines
        assert!(result.text.contains("untagged content"));
        assert!(result.text.contains("This is the introduction."));
        assert!(result.text.contains("This is the main content."));
        assert!(result.text.contains("Debug information"));
        // Should NOT contain tag directives
        assert!(!result.text.contains("tag::intro"));
        assert!(!result.text.contains("end::intro"));
        Ok(())
    }

    #[test]
    fn test_include_with_nested_tag() -> Result<(), Error> {
        let preprocessor = Preprocessor;
        let path = Path::new("fixtures/preprocessor/include_nested_tag.adoc");
        let options = Options::default();

        let result = preprocessor.process_file(path, &options)?;

        // Should contain only the nested content
        assert!(result.text.contains("This is nested within main."));
        // Should NOT contain main content outside nested
        assert!(!result.text.contains("This is the main content."));
        assert!(!result.text.contains("Back to main content."));
        Ok(())
    }

    #[test]
    fn test_include_select_untagged_only() -> Result<(), Error> {
        let preprocessor = Preprocessor;
        let path = Path::new("fixtures/preprocessor/include_untagged_only.adoc");
        let options = Options::default();

        let result = preprocessor.process_file(path, &options)?;

        // Should contain only untagged content
        assert!(result.text.contains("untagged content at the beginning"));
        assert!(result.text.contains("More untagged content"));
        assert!(result.text.contains("Final untagged content"));
        // Should NOT contain any tagged content
        assert!(!result.text.contains("This is the introduction"));
        assert!(!result.text.contains("This is the main content"));
        assert!(!result.text.contains("Debug information"));
        Ok(())
    }

    #[test]
    fn test_include_tag_with_lines() -> Result<(), Error> {
        let preprocessor = Preprocessor;
        let path = Path::new("fixtures/preprocessor/include_tag_with_lines.adoc");
        let options = Options::default();

        let result = preprocessor.process_file(path, &options)?;

        // When combining tag= and lines=, the lines= attribute refers to
        // line numbers in the ORIGINAL file, not the filtered result.
        // tag=intro selects lines 4-5 (content between tag directives)
        // lines=4 selects only line 4 from the original file
        // The intersection is just line 4: "This is the introduction."
        assert!(result.text.contains("This is the introduction."));
        // Line 5 is not in lines=4, so it should NOT be included
        assert!(!result.text.contains("It has multiple lines."));
        Ok(())
    }

    #[test]
    fn test_nested_include_relative_paths() -> Result<(), Error> {
        // Tests that nested includes resolve paths relative to their parent file.
        // Structure:
        //   nested_include_main.adoc
        //     -> includes subdir/middle.adoc
        //         -> includes inner.adoc (relative to subdir/)
        let preprocessor = Preprocessor;
        let path = Path::new("fixtures/preprocessor/nested_include_main.adoc");
        let options = Options::default();

        let result = preprocessor.process_file(path, &options)?;

        // Should contain content from main file
        assert!(result.text.contains("= Nested Include Test"));
        // Should contain content from subdir/middle.adoc
        assert!(result.text.contains("This is middle content."));
        // Should contain content from subdir/inner.adoc (resolved relative to subdir/)
        assert!(
            result.text.contains("This is inner content from subdir."),
            "Nested include failed to resolve relative path. Got: {}",
            result.text
        );
        Ok(())
    }
}
