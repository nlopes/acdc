use std::process::exit;

use acdc_parser::SourceLocation;
use miette::{Diagnostic, NamedSource, Report, SourceSpan};

/// Rich error wrapper for beautiful miette display with source code
#[derive(Debug, Diagnostic, thiserror::Error)]
#[error("{message}")]
#[diagnostic()]
pub(crate) struct RichError<'a> {
    message: String,

    #[help]
    advice: Option<&'a str>,

    #[source_code]
    src: NamedSource<String>,

    #[label("{position_advice}")]
    span: SourceSpan,
    position_advice: String,
}

fn source_span_from_source_location(loc: &SourceLocation) -> SourceSpan {
    match &loc.positioning {
        acdc_parser::Positioning::Location(location) => {
            let start_offset = location.absolute_start;
            let length = location.absolute_end - location.absolute_start;
            SourceSpan::new(start_offset.into(), length)
        }
        acdc_parser::Positioning::Position(position) => {
            // Single character span at the position
            SourceSpan::new(position.offset.into(), 1)
        }
    }
}

pub(crate) fn display<E: std::error::Error + 'static>(e: &E) -> Report {
    if let Some(parser_error) = acdc_converters_common::find_parser_error(e)
        && let Some(source_location) = parser_error.source_location()
        && let Some(path) = &source_location.file
        /* Lazy-load file content only if we have a file path */
        && let Ok(source_str) = std::fs::read_to_string(path)
    {
        let advice = parser_error.advice();
        let named_source = NamedSource::new(path.display().to_string(), source_str);
        let source_span = source_span_from_source_location(source_location);
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
