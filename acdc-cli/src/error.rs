// In Rust 1.92 without this, miette generates warnings about unused assignments.
//
// See issue https://github.com/zkat/miette/issues/458 and PR
// https://github.com/zkat/miette/pull/459 for more details.
#![allow(unused_assignments)]

use std::process::exit;

use acdc_parser::SourceLocation;
use miette::{Diagnostic, NamedSource, Report, SourceSpan};

/// Rich error wrapper for beautiful miette display with source code
#[derive(Debug, Diagnostic, thiserror::Error)]
#[error("{message}")]
#[diagnostic()]
pub(crate) struct RichError {
    message: String,

    #[help]
    advice: Option<&'static str>,

    #[source_code]
    src: NamedSource<String>,

    #[label("{position_advice}")]
    span: SourceSpan,
    position_advice: String,
}

fn source_span_from_source_location(loc: &SourceLocation, source: &str) -> SourceSpan {
    match &loc.positioning {
        acdc_parser::Positioning::Location(location) => {
            let start_offset = location.absolute_start;
            let length = location.absolute_end - location.absolute_start;
            SourceSpan::new(start_offset.into(), length)
        }
        acdc_parser::Positioning::Position(position) => {
            // Calculate byte offset from line/column
            let offset = calculate_offset_from_position(source, position.line, position.column);
            SourceSpan::new(offset.into(), 1)
        }
    }
}

/// Calculate byte offset from line and column numbers (both 1-indexed).
fn calculate_offset_from_position(source: &str, line: usize, column: usize) -> usize {
    let mut current_line = 1;

    for (idx, ch) in source.char_indices() {
        if current_line == line {
            // Found the target line, now count columns (1-indexed)
            let line_start = idx;
            let mut col = 1;
            for (col_idx, col_ch) in source[line_start..].char_indices() {
                if col == column {
                    return line_start + col_idx;
                }
                if col_ch == '\n' {
                    break;
                }
                col += 1;
            }
            // Column not found on line, return end of line
            return line_start
                + source[line_start..]
                    .find('\n')
                    .unwrap_or(source.len() - line_start);
        }
        if ch == '\n' {
            current_line += 1;
        }
    }

    // Line not found, return end of source
    source.len().saturating_sub(1)
}

pub(crate) fn display<E: std::error::Error + 'static>(e: &E) -> Report {
    if let Some(parser_error) = acdc_converters_common::find_parser_error(e)
        && let Some(source_location) = parser_error.source_location()
        && let Some(path) = &source_location.file
        /* Lazy-load file content only if we have a file path */
        && let Ok(source_str) = std::fs::read_to_string(path)
    {
        let advice = parser_error.advice();
        let source_span = source_span_from_source_location(source_location, &source_str);
        let named_source = NamedSource::new(path.display().to_string(), source_str);
        let position_advice = match &source_location.positioning {
            acdc_parser::Positioning::Location(location) => {
                format!(
                    "error occurred here (line {}, column {})",
                    location.start.line, location.start.column
                )
            }
            acdc_parser::Positioning::Position(position) => {
                format!(
                    "error occurred here (line {}, column {})",
                    position.line, position.column
                )
            }
        };
        let rich_error = RichError {
            message: parser_error.to_string(),
            advice,
            src: named_source,
            span: source_span,
            position_advice,
        };
        return Report::new(rich_error);
    }

    // Fallback: No parser error with location found, or couldn't read file, display normally
    eprintln!("Error: {e}");
    exit(1);
}
