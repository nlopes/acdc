use std::{error::Error, path::PathBuf, process::exit};

use acdc_parser::{Error as ParserError, Location};
use miette::{Diagnostic, NamedSource, SourceSpan};

/// Rich error wrapper for beautiful miette display with source code
#[derive(Debug, Diagnostic, thiserror::Error)]
#[error("{message}")]
#[diagnostic()]
pub(crate) struct RichError {
    message: String,

    #[help]
    advice: String,

    #[source_code]
    src: NamedSource<String>,

    #[label("{position_advice}")]
    span: SourceSpan,
    position_advice: String,
}

fn source_span_from_location(location: &Location) -> SourceSpan {
    let start_offset = location.absolute_start;
    let length = location.absolute_end - location.absolute_start;

    SourceSpan::new(start_offset.into(), length)
}

pub(crate) fn display<E: Error + 'static>(e: E, source_context: Option<&(PathBuf, String)>) {
    let mut current_error: &dyn Error = &e;
    // Check if this error wraps a parser error by walking the source chain
    while let Some(current) = current_error.source()
        && let Some(parser_error) = current.downcast_ref::<ParserError>()
    {
        // set to the next error in the chain
        current_error = current;

        // Try to create a rich report with source code if available
        if let (Some((path, source)), Some(location)) = (source_context, parser_error.location()) {
            let advice = parser_error.advice().unwrap_or_default().to_string();

            // Create a named source for the file
            let named_source = NamedSource::new(path.display().to_string(), source.clone());

            // Convert location to SourceSpan (offset, length)
            let source_span = source_span_from_location(location);

            // Create label showing where error occurred
            let line = location.start.line;
            let column = location.start.column;
            let position_advice = format!("error occurred here (line {line}, column {column})");

            // Create rich error with source code
            let rich_error = RichError {
                message: parser_error.to_string(),
                advice,
                src: named_source,
                span: source_span,
                position_advice,
            };

            eprint!("{:?}", miette::Report::new(rich_error));
            exit(1);
        } else {
            // // Fallback to simpler display without source
            eprintln!("  × {parser_error}");
            if let Some(advice) = parser_error.advice() {
                eprintln!("  help: {advice}");
            }
        }
    }
    // No parser error found, display normally
    eprintln!("Error: {e}");
    exit(1);
}
