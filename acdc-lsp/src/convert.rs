//! Type conversions between acdc-parser and LSP types

use acdc_parser::Location;
use tower_lsp::lsp_types::{Position, Range};

/// Convert acdc-parser Location to LSP Range
///
/// Note: acdc-parser uses 1-indexed lines/columns, LSP uses 0-indexed
#[must_use]
#[allow(clippy::cast_possible_truncation)] // Line/column numbers won't exceed u32::MAX
pub fn location_to_range(loc: &Location) -> Range {
    Range {
        start: Position {
            line: loc.start.line.saturating_sub(1) as u32,
            character: loc.start.column.saturating_sub(1) as u32,
        },
        end: Position {
            line: loc.end.line.saturating_sub(1) as u32,
            character: loc.end.column.saturating_sub(1) as u32,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_location_to_range_default_is_zero() {
        // Location::default() has all zeros
        let loc = Location::default();
        let range = location_to_range(&loc);

        // 0-indexed: 0.saturating_sub(1) = 0
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 0);
    }
}
