// In Rust 1.92 without this, miette generates warnings about unused assignments.
//
// See issue https://github.com/zkat/miette/issues/458 and PR
// https://github.com/zkat/miette/pull/459 for more details.
#![allow(unused_assignments)]

use std::{path::Path, process::exit};

use acdc_converters_core::Warning as ConverterWarning;
#[cfg(feature = "lint")]
use acdc_lint::{LintDiagnostic, LintLevel};
use acdc_parser::{SourceLocation, Warning as ParserWarning};
use miette::{Diagnostic, NamedSource, Report, SourceSpan};
#[cfg(feature = "lint")]
use miette::{LabeledSpan, Severity};

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

#[cfg(feature = "lint")]
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub(crate) struct FullLintDiagnostic {
    message: String,
    code: String,
    severity: Severity,
    advice: Option<String>,
    src: NamedSource<String>,
    span: SourceSpan,
    position_advice: String,
}

#[cfg(feature = "lint")]
impl Diagnostic for FullLintDiagnostic {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(&self.code))
    }

    fn severity(&self) -> Option<Severity> {
        Some(self.severity)
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.advice
            .as_ref()
            .map(|help| Box::new(help) as Box<dyn std::fmt::Display + 'a>)
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&self.src)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        Some(Box::new(std::iter::once(
            LabeledSpan::new_primary_with_span(Some(self.position_advice.clone()), self.span),
        )))
    }
}

#[cfg(feature = "lint")]
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub(crate) struct CompactLintDiagnostic {
    message: String,
    code: String,
    severity: Severity,
    advice: Option<String>,
}

#[cfg(feature = "lint")]
impl Diagnostic for CompactLintDiagnostic {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(&self.code))
    }

    fn severity(&self) -> Option<Severity> {
        Some(self.severity)
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.advice
            .as_ref()
            .map(|help| Box::new(help) as Box<dyn std::fmt::Display + 'a>)
    }
}

fn source_span_from_source_location(loc: &SourceLocation, source: &str) -> SourceSpan {
    // `absolute_start`/`absolute_end` index the *preprocessed* buffer, which diverges
    // from `source` (the original file rendered here) once includes, conditionals, or
    // dropped comments shift content — using them directly can run the span past the
    // file end (miette `OutOfBounds`). Derive the start from the source-relative
    // line/column instead, and clamp the length so it can never exceed the remaining
    // bytes in `source`. A zero-width (point) location renders as a 1-byte span.
    let location = &loc.location;
    let start_offset =
        calculate_offset_from_position(source, location.start.line, location.start.column);
    let preprocessed_len = location
        .absolute_end
        .saturating_sub(location.absolute_start);
    let length = preprocessed_len
        .min(source.len().saturating_sub(start_offset))
        .max(1);
    SourceSpan::new(start_offset.into(), length)
}

fn source_location_line_column(loc: &SourceLocation) -> (u32, u32) {
    (loc.location.start.line, loc.location.start.column)
}

/// Calculate byte offset from line and column numbers (both 1-indexed).
fn calculate_offset_from_position(source: &str, line: u32, column: u32) -> usize {
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
        let (line, column) = source_location_line_column(loc);
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

#[cfg(feature = "lint")]
fn severity_for_lint_level(level: LintLevel) -> Severity {
    match level {
        LintLevel::Allow | LintLevel::Warn => Severity::Warning,
        LintLevel::Deny | LintLevel::Forbid => Severity::Error,
    }
}

#[cfg(feature = "lint")]
pub(crate) fn lint_diagnostic_report(
    diagnostic: &LintDiagnostic,
    fallback_file: Option<&Path>,
    fallback_source_name: Option<&str>,
    fallback_source: Option<&str>,
) -> Report {
    let message = diagnostic.message().to_owned();
    let code = diagnostic.lint().name().to_owned();
    let severity = severity_for_lint_level(diagnostic.level());
    let advice = diagnostic.help().map(str::to_string);

    let full = diagnostic.location().and_then(|loc| {
        let (name, source_str) = match loc.file.as_deref().or(fallback_file) {
            Some(path) => (
                path.display().to_string(),
                std::fs::read_to_string(path).ok()?,
            ),
            None => (
                fallback_source_name.unwrap_or("<stdin>").to_owned(),
                fallback_source?.to_owned(),
            ),
        };
        let span = source_span_from_source_location(loc, &source_str);
        let (line, column) = source_location_line_column(loc);
        Some(FullLintDiagnostic {
            message: message.clone(),
            code: code.clone(),
            severity,
            advice: advice.clone(),
            src: NamedSource::new(name, source_str),
            span,
            position_advice: format!(
                "{} triggered here (line {line}, column {column})",
                diagnostic.lint()
            ),
        })
    });

    match full {
        Some(full) => Report::new(full),
        None => Report::new(CompactLintDiagnostic {
            message,
            code,
            severity,
            advice,
        }),
    }
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
        let (line, column) = source_location_line_column(source_location);
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
