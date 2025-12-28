//! Type conversions between acdc-parser and LSP types

use acdc_parser::Location;
use tower_lsp::lsp_types::{Position, Range};

/// Convert usize to u32 for LSP types, saturating at `u32::MAX`.
///
/// LSP uses u32 for line/column numbers while the parser uses usize.
/// In practice, source files won't have 4 billion+ lines/columns,
/// so saturation is a safe fallback.
fn to_lsp_u32(val: usize) -> u32 {
    val.try_into().unwrap_or(u32::MAX)
}

/// Convert LSP position to byte offset in source text.
///
/// Returns `None` if the position is out of bounds.
#[must_use]
pub fn position_to_offset(source: &str, position: Position) -> Option<usize> {
    let target_line = position.line as usize;
    let target_char = position.character as usize;

    let mut current_offset = 0;
    for (line_idx, line) in source.lines().enumerate() {
        if line_idx == target_line {
            // Count characters (not bytes) up to column
            let char_offset: usize = line.chars().take(target_char).map(char::len_utf8).sum();
            return Some(current_offset + char_offset);
        }
        current_offset += line.len() + 1; // +1 for newline
    }

    // Position is beyond end of document
    None
}

/// Check if a byte offset falls within a `Location`.
#[must_use]
pub fn offset_in_location(offset: usize, location: &Location) -> bool {
    offset >= location.absolute_start && offset < location.absolute_end
}

/// Convert acdc-parser Location to LSP Range
///
/// Note: acdc-parser uses 1-indexed lines/columns, LSP uses 0-indexed
#[must_use]
pub fn location_to_range(loc: &Location) -> Range {
    Range {
        start: Position {
            line: to_lsp_u32(loc.start.line.saturating_sub(1)),
            character: to_lsp_u32(loc.start.column.saturating_sub(1)),
        },
        end: Position {
            line: to_lsp_u32(loc.end.line.saturating_sub(1)),
            character: to_lsp_u32(loc.end.column.saturating_sub(1)),
        },
    }
}

/// Convert a parser Position to an LSP Position
///
/// Note: acdc-parser uses 1-indexed, LSP uses 0-indexed
#[must_use]
pub fn parser_position_to_lsp(pos: &acdc_parser::Position) -> Position {
    Position {
        line: to_lsp_u32(pos.line.saturating_sub(1)),
        character: to_lsp_u32(pos.column.saturating_sub(1)),
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

    #[test]
    fn test_position_to_offset_simple() {
        let source = "line 1\nline 2\nline 3";
        let pos = Position {
            line: 1,
            character: 0,
        };

        let offset = position_to_offset(source, pos);
        assert_eq!(offset, Some(7)); // "line 1\n" = 7 bytes
    }

    #[test]
    fn test_position_to_offset_with_unicode() {
        let source = "héllo\nwörld";
        let pos = Position {
            line: 1,
            character: 0,
        };

        let offset = position_to_offset(source, pos);
        assert_eq!(offset, Some(7)); // "héllo\n" = 7 bytes (é is 2 bytes)
    }

    #[test]
    fn test_offset_in_location() {
        // Use mutation since Location is #[non_exhaustive]
        let mut location = Location::default();
        location.absolute_start = 10;
        location.absolute_end = 20;
        location.start.line = 1;
        location.start.column = 1;
        location.end.line = 1;
        location.end.column = 11;

        assert!(!offset_in_location(9, &location));
        assert!(offset_in_location(10, &location));
        assert!(offset_in_location(15, &location));
        assert!(!offset_in_location(20, &location)); // end is exclusive
    }
}
