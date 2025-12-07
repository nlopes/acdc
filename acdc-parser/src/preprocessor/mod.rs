//! The preprocessor module is responsible for processing the input document and expanding include directives.
use std::fmt::Write as _;
use std::path::Path;

use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE};

use crate::{
    Options,
    error::{Error, Positioning, SourceLocation},
    model::Position,
};

mod attribute;
mod conditional;
mod include;

use include::Include;

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
    ) -> Result<String, Error> {
        let mut input = String::new();
        reader.read_to_string(&mut input).map_err(|e| {
            tracing::error!(error=?e, "failed to read from reader");
            e
        })?;
        self.process(&input, options)
    }

    #[tracing::instrument]
    pub(crate) fn process(&self, input: &str, options: &Options) -> Result<String, Error> {
        self.process_either(input, None, options)
    }

    #[tracing::instrument(skip(file_path))]
    pub(crate) fn process_file<P: AsRef<Path>>(
        &self,
        file_path: P,
        options: &Options,
    ) -> Result<String, Error> {
        if file_path.as_ref().parent().is_some() {
            // Use read_and_decode_file to support UTF-8, UTF-16 LE, and UTF-16 BE with BOM
            let input = read_and_decode_file(file_path.as_ref(), None)?;
            self.process_either(&input, Some(file_path.as_ref()), options)
        } else {
            Err(Error::InvalidIncludePath(
                Box::new(Self::create_source_location(1, Some(file_path.as_ref()))),
                file_path.as_ref().to_path_buf(),
            ))
        }
    }

    /// Process an include directive
    #[tracing::instrument]
    fn process_include(
        line: &str,
        line_number: usize,
        current_offset: usize,
        file_parent: Option<&Path>,
        options: &Options,
    ) -> Result<Option<Vec<String>>, Error> {
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
            tracing::error!("file parent is missing - include directive cannot be processed");
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

    #[tracing::instrument]
    fn process_either(
        &self,
        input: &str,
        file_parent: Option<&Path>,
        options: &Options,
    ) -> Result<String, Error> {
        let input = Preprocessor::normalize(input);
        let mut options = options.clone();
        // Pre-allocate output Vec with estimated line count
        let estimated_lines = input.lines().count();
        let mut output = Vec::with_capacity(estimated_lines);
        let mut lines = input.lines().peekable();
        let mut line_number = 1; // Track the current line number (1-indexed)
        let mut current_offset = 0; // Track absolute byte offset in document

        // Track verbatim block state for comment filtering
        let mut in_verbatim_block = false;
        let mut current_delimiter: Option<&str> = None;

        while let Some(line) = lines.next() {
            if line.starts_with(':') && (line.ends_with(" + \\") || line.ends_with(" \\")) {
                // Pre-allocate with initial line length as estimate
                let mut attribute_content = String::with_capacity(line.len() * 2);
                if line.ends_with(" + \\") {
                    attribute_content.push_str(line);
                    attribute_content.push('\n');
                } else if line.ends_with(" \\") {
                    attribute_content.push_str(line.trim_end_matches('\\'));
                }
                Self::process_continuation(&mut attribute_content, &mut lines, &mut line_number);
                attribute::parse_line(&mut options.document_attributes, attribute_content.as_str());
                output.push(attribute_content);
                continue;
            } else if line.starts_with(':') {
                attribute::parse_line(&mut options.document_attributes, line.trim());
            }
            // Check for verbatim block delimiters to track when to preserve comments
            if let Some(delimiter_type) = Self::is_verbatim_delimiter(line) {
                if in_verbatim_block && Some(delimiter_type) == current_delimiter {
                    // Closing delimiter - exit verbatim block
                    tracing::trace!(?delimiter_type, "Closing verbatim block");
                    in_verbatim_block = false;
                    current_delimiter = None;
                } else if !in_verbatim_block {
                    // Opening delimiter - enter verbatim block
                    tracing::trace!(?delimiter_type, "Opening verbatim block");
                    in_verbatim_block = true;
                    current_delimiter = Some(delimiter_type);
                }
                output.push(line.to_string());
            }
            // Taken from
            // https://github.com/asciidoctor/asciidoctor/blob/306111f480e2853ba59107336408de15253ca165/lib/asciidoctor/reader.rb#L604
            // while following the specs at
            // https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/spec/outline.adoc?ref_type=heads#user-content-preprocessor

            // Preserve single-line comments (lines starting with //) for the parser to handle.
            // Comments serve as semantic list separators in AsciiDoc, so the parser needs to see them.
            // The grammar's comment() rule handles skipping them during parsing.
            else if line.starts_with("//") {
                tracing::trace!(line, "Preserving comment line for parser");
                output.push(line.to_string());
            } else if line.ends_with(']') && !line.starts_with('[') && line.contains("::") {
                if line.starts_with("\\include")
                    || line.starts_with("\\ifdef")
                    || line.starts_with("\\ifndef")
                    || line.starts_with("\\ifeval")
                {
                    // Return the directive as is
                    output.push(line[1..].to_string());
                } else if line.starts_with("ifdef")
                    || line.starts_with("ifndef")
                    || line.starts_with("ifeval")
                {
                    let current_line = line_number;
                    if let Some(content) = Self::process_conditional(
                        line,
                        &mut lines,
                        &mut line_number,
                        current_line,
                        current_offset,
                        file_parent,
                        &options.document_attributes,
                    )? {
                        output.push(content);
                    }
                } else if line.starts_with("include") {
                    if let Some(lines) = Self::process_include(
                        line,
                        line_number,
                        current_offset,
                        file_parent,
                        &options,
                    )? {
                        output.extend(lines);
                    }
                } else {
                    // Return the directive as is
                    output.push(line.to_string());
                }
            } else {
                // Return the line as is
                output.push(line.to_string());
            }
            // Move to next line: account for line length + newline character
            current_offset += line.len() + 1;
            line_number += 1;
        }

        Ok(output.join("\n"))
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
        let output = Preprocessor.process(input, &options)?;
        assert_eq!(output, ":attribute: value\n\ncontent\n");
        Ok(())
    }

    #[test]
    fn test_good_endif_directive() -> Result<(), Error> {
        let options = Options::default();
        let input = ":asdf:

ifdef::asdf[]
content
endif::asdf[]";
        let output = Preprocessor.process(input, &options)?;
        assert_eq!(output, ":asdf:\n\ncontent\n");
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
        assert!(result.contains("= Main Document"));
        assert!(result.contains("This is included content."));
        assert!(result.contains("With special characters: é, ñ, ü."));
        assert!(result.contains("After include."));
        Ok(())
    }
}
