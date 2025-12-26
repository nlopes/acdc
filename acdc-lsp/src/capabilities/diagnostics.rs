//! Diagnostics: convert acdc-parser errors to LSP diagnostics

use acdc_parser::{Error, Positioning};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

/// Convert acdc-parser Error to LSP Diagnostic
#[must_use]
pub fn error_to_diagnostic(error: &Error) -> Diagnostic {
    let range = error
        .source_location()
        .map(|source_loc| positioning_to_range(&source_loc.positioning))
        .unwrap_or_default();

    let message = error.to_string();

    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("acdc".to_string()),
        message,
        ..Default::default()
    }
}

/// Convert acdc-parser Positioning to LSP Range
///
/// Note: acdc-parser uses 1-indexed lines/columns, LSP uses 0-indexed
#[allow(clippy::cast_possible_truncation)] // Line/column numbers won't exceed u32::MAX
fn positioning_to_range(pos: &Positioning) -> Range {
    match pos {
        Positioning::Location(loc) => Range {
            start: Position {
                line: loc.start.line.saturating_sub(1) as u32,
                character: loc.start.column.saturating_sub(1) as u32,
            },
            end: Position {
                line: loc.end.line.saturating_sub(1) as u32,
                character: loc.end.column.saturating_sub(1) as u32,
            },
        },
        Positioning::Position(p) => {
            let pos = Position {
                line: p.line.saturating_sub(1) as u32,
                character: p.column.saturating_sub(1) as u32,
            };
            Range {
                start: pos,
                end: pos,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::Location;

    #[test]
    fn test_positioning_converts_to_zero_indexed() {
        // Use Location::default() since Location is non_exhaustive
        let loc = Location::default();
        let range = positioning_to_range(&Positioning::Location(loc));

        // Default Location has line=0, column=0
        // 0.saturating_sub(1) = 0
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 0);
    }
}
