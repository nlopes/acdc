//! Type conversions between acdc-parser and LSP types

use acdc_parser::{Location, SourceLocation};
use tower_lsp_server::ls_types::{Position, Range, Uri};

/// Convert usize to u32 for LSP types, saturating at `u32::MAX`.
///
/// LSP uses u32 for line/column numbers while the parser uses usize.
/// In practice, source files won't have 4 billion+ lines/columns,
/// so saturation is a safe fallback.
#[must_use]
pub(crate) fn to_lsp_u32(val: usize) -> u32 {
    val.try_into().unwrap_or(u32::MAX)
}

/// Convert LSP position to byte offset in source text.
///
/// Returns `None` if the position is out of bounds.
#[must_use]
pub(crate) fn position_to_offset(source: &str, position: Position) -> Option<usize> {
    let target_line = position.line as usize;
    let target_char = position.character as usize;

    let mut current_offset = 0;
    for (line_idx, line_with_ending) in source.split_inclusive('\n').enumerate() {
        if line_idx == target_line {
            let line = line_with_ending
                .strip_suffix("\r\n")
                .or_else(|| line_with_ending.strip_suffix('\n'))
                .unwrap_or(line_with_ending);
            let mut units = 0;
            for (byte_offset, character) in line.char_indices() {
                if units == target_char {
                    return Some(current_offset + byte_offset);
                }
                units += character.len_utf16();
                if units > target_char {
                    return None;
                }
            }
            return (units == target_char).then_some(current_offset + line.len());
        }
        current_offset += line_with_ending.len();
    }

    (position.character == 0 && target_line == source.bytes().filter(|&byte| byte == b'\n').count())
        .then_some(source.len())
}

/// Check a cursor offset in the primary input against an AST location.
///
/// The parser's `absolute_end` is inclusive (points to the last byte of the
/// span), so we use `<=` for the upper bound.
#[must_use]
pub(crate) fn offset_in_location(offset: usize, location: &Location) -> bool {
    location
        .start
        .file
        .as_ref()
        .is_none_or(|chain| chain.is_empty())
        && location
            .end
            .file
            .as_ref()
            .is_none_or(|chain| chain.is_empty())
        && offset >= location.absolute_start
        && offset <= location.absolute_end
}

/// Resolve a single-file span to a URI. Unknown include sources have no fallback.
pub(crate) fn source_uri(document_uri: &Uri, source: &SourceLocation) -> Option<Uri> {
    if source.location.start.file != source.location.end.file {
        return None;
    }
    match &source.file {
        Some(path) => Uri::from_file_path(path),
        None if source
            .location
            .start
            .file
            .as_ref()
            .is_none_or(|chain| chain.is_empty()) =>
        {
            Some(document_uri.clone())
        }
        None => None,
    }
}

/// Convert acdc-parser Location to LSP Range
///
/// Note: acdc-parser uses 1-indexed lines/columns with inclusive end,
/// while LSP uses 0-indexed lines/characters with exclusive end.
/// We convert start by subtracting 1, and end by keeping the column as-is
/// (subtract 1 for 1-indexed→0-indexed, then add 1 for inclusive→exclusive).
#[must_use]
pub(crate) fn location_to_range(loc: &Location) -> Range {
    Range {
        start: Position {
            line: loc.start.line.saturating_sub(1),
            character: loc.start.column.saturating_sub(1),
        },
        end: Position {
            line: loc.end.line.saturating_sub(1),
            character: loc.end.column,
        },
    }
}

/// Resolve a relative path against a document URI's directory.
///
/// Uses RFC 3986 reference resolution via `fluent_uri`, which correctly
/// handles `..` and `.` segments, percent-encoding, and edge cases.
#[must_use]
pub(crate) fn resolve_relative_uri(doc_uri: &Uri, relative_path: &str) -> Option<Uri> {
    let reference = fluent_uri::UriRef::parse(relative_path).ok()?;
    let resolved = reference.resolve_against(doc_uri).ok()?;
    resolved.as_str().parse().ok()
}

/// Extract the filename from a URI string (last path segment).
#[must_use]
pub(crate) fn uri_filename(uri: &Uri) -> &str {
    uri.as_str()
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(uri.as_str())
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
    fn position_to_offset_uses_utf16_and_preserves_crlf() {
        let text = "😀 first\r\nnext\r\n";
        assert_eq!(position_to_offset(text, Position::new(0, 1)), None);
        assert_eq!(position_to_offset(text, Position::new(0, 2)), Some(4));
        assert_eq!(position_to_offset(text, Position::new(0, 99)), None);
        assert_eq!(position_to_offset(text, Position::new(1, 0)), Some(12));
        assert_eq!(
            position_to_offset(text, Position::new(2, 0)),
            Some(text.len())
        );
    }

    #[test]
    fn source_uri_rejects_unknown_and_cross_file_spans() -> Result<(), Box<dyn std::error::Error>> {
        let uri: Uri = "file:///book.adoc".parse()?;
        let mut location = Location::default();
        location.start.file = Some(std::sync::Arc::new(vec!["unknown.adoc".into()]));
        location.end.file = location.start.file.clone();
        let mut source = SourceLocation::at_location(None, location);
        assert_eq!(source_uri(&uri, &source), None);
        source.file = uri.to_file_path().map(std::borrow::Cow::into_owned);
        source.location.end.file = None;
        assert_eq!(source_uri(&uri, &source), None);
        Ok(())
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
        assert!(offset_in_location(20, &location)); // end is inclusive
        assert!(!offset_in_location(21, &location));
    }

    #[test]
    fn test_resolve_relative_uri_simple() -> Result<(), Box<dyn std::error::Error>> {
        let doc: Uri = "file:///docs/main.adoc".parse()?;
        let resolved = resolve_relative_uri(&doc, "chapter2.adoc");
        assert_eq!(
            resolved.map(|u| u.as_str().to_string()),
            Some("file:///docs/chapter2.adoc".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_resolve_relative_uri_parent_traversal() -> Result<(), Box<dyn std::error::Error>> {
        let doc: Uri = "file:///docs/sub/main.adoc".parse()?;
        let resolved = resolve_relative_uri(&doc, "../other.adoc");
        assert_eq!(
            resolved.map(|u| u.as_str().to_string()),
            Some("file:///docs/other.adoc".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_resolve_relative_uri_dot_segment() -> Result<(), Box<dyn std::error::Error>> {
        let doc: Uri = "file:///docs/main.adoc".parse()?;
        let resolved = resolve_relative_uri(&doc, "./chapter.adoc");
        assert_eq!(
            resolved.map(|u| u.as_str().to_string()),
            Some("file:///docs/chapter.adoc".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_uri_filename() -> Result<(), Box<dyn std::error::Error>> {
        let uri: Uri = "file:///docs/main.adoc".parse()?;
        assert_eq!(uri_filename(&uri), "main.adoc");
        Ok(())
    }
}
