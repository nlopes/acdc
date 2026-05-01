// In Rust 1.92 without this, miette generates warnings about unused assignments.
//
// See issue https://github.com/zkat/miette/issues/458 and PR
// https://github.com/zkat/miette/pull/459 for more details.
#![allow(unused_assignments)]

use std::{path::Path, process::exit};

use acdc_converters_core::Warning as ConverterWarning;
use acdc_parser::{SourceLocation, Warning as ParserWarning};
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

/// Rich warning wrapper: same shape as `RichError` but with `severity = Warning` so
/// miette renders it in the warning palette (yellow, `⚠`) rather than the error palette
/// (red, `✗`).
#[derive(Debug, Diagnostic, thiserror::Error)]
#[error("{message}")]
#[diagnostic(severity(Warning))]
pub(crate) struct RichWarning {
    message: String,

    #[help]
    advice: Option<String>,

    #[source_code]
    src: NamedSource<String>,

    #[label("{position_advice}")]
    span: SourceSpan,
    position_advice: String,
}

/// Minimal warning with no attached source span. Used when the warning has no location,
/// or when we can't read the source file to build a rich span.
#[derive(Debug, Diagnostic, thiserror::Error)]
#[error("{message}")]
#[diagnostic(severity(Warning))]
pub(crate) struct PlainWarning {
    message: String,

    #[help]
    advice: Option<String>,
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

fn positioning_line_column(positioning: &acdc_parser::Positioning) -> (usize, usize) {
    match positioning {
        acdc_parser::Positioning::Location(location) => {
            (location.start.line, location.start.column)
        }
        acdc_parser::Positioning::Position(position) => (position.line, position.column),
    }
}

/// Calculate byte offset from line and column numbers (both 1-indexed).
fn calculate_offset_from_position(source: &str, line: usize, column: usize) -> usize {
    let mut current_line = 1;

    for (idx, ch) in source.char_indices() {
        if current_line == line {
            // Found the target line, now count columns (1-indexed)
            let line_start = idx;
            for (col, (col_idx, col_ch)) in (1..).zip(source[line_start..].char_indices()) {
                if col == column {
                    return line_start + col_idx;
                }
                if col_ch == '\n' {
                    break;
                }
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

/// Build a miette `Report` from extracted warning fields.
///
/// Tries to load the referenced source file and produce a `RichWarning` with a
/// span/snippet; falls back to a source-less `PlainWarning` when the warning has
/// no location, no file path, or the file can't be read. `fallback_file`
/// (typically the file being processed) is used when the warning's own
/// `SourceLocation` has no path; pass `None` for stdin input.
fn build_warning_report(
    message: String,
    advice: Option<String>,
    location: Option<&SourceLocation>,
    fallback_file: Option<&Path>,
) -> Report {
    let rich = location.and_then(|loc| {
        let path = loc.file.as_deref().or(fallback_file)?;
        let source_str = std::fs::read_to_string(path).ok()?;
        let span = source_span_from_source_location(loc, &source_str);
        let (line, column) = positioning_line_column(&loc.positioning);
        Some(RichWarning {
            message: message.clone(),
            advice: advice.clone(),
            src: NamedSource::new(path.display().to_string(), source_str),
            span,
            position_advice: format!("warning occurred here (line {line}, column {column})"),
        })
    });

    match rich {
        Some(rich) => Report::new(rich),
        None => Report::new(PlainWarning { message, advice }),
    }
}

/// Render a parser warning as a miette `Report`.
pub(crate) fn parser_warning_report(
    warning: &ParserWarning,
    fallback_file: Option<&Path>,
) -> Report {
    build_warning_report(
        warning.kind.to_string(),
        warning.advice().map(str::to_string),
        warning.source_location(),
        fallback_file,
    )
}

/// Render a converter warning as a miette `Report`.
pub(crate) fn converter_warning_report(
    warning: &ConverterWarning,
    fallback_file: Option<&Path>,
) -> Report {
    build_warning_report(
        warning.to_string(),
        warning.advice().map(str::to_string),
        warning.source_location(),
        fallback_file,
    )
}

pub(crate) fn display<E: std::error::Error + 'static>(e: &E) -> Report {
    if let Some(parser_error) = acdc_converters_core::find_parser_error(e)
        && let Some(source_location) = parser_error.source_location()
        && let Some(path) = &source_location.file
        /* Lazy-load file content only if we have a file path */
        /*
           NOTE: this might be an issue for very large files, but in practice we expect the source
           files to be reasonably sized, and this allows us to show rich snippets for errors that
           have file paths but no source content attached. I might regret leaving this as is.
        */
        && let Ok(source_str) = std::fs::read_to_string(path)
    {
        let advice = parser_error.advice();
        let span = source_span_from_source_location(source_location, &source_str);
        let named_source = NamedSource::new(path.display().to_string(), source_str);
        let (line, column) = positioning_line_column(&source_location.positioning);
        let position_advice = format!("error occurred here (line {line}, column {column})");
        let rich_error = RichError {
            message: parser_error.to_string(),
            advice,
            src: named_source,
            span,
            position_advice,
        };
        return Report::new(rich_error);
    }

    // Fallback: No parser error with location found, or couldn't read file, display normally
    eprintln!("Error: {e}");
    exit(1);
}
