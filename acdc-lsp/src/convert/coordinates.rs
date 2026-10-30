use acdc_parser::Location;
use tower_lsp_server::ls_types::{Position, Range};

use super::to_lsp_u32;

/// Convert an exclusive byte range in original source to UTF-16 wire coordinates.
/// Invalid UTF-8 boundaries and out-of-bounds ranges return `None`.
pub(crate) fn byte_range_to_utf16_range(
    source: &str,
    bytes: std::ops::Range<usize>,
) -> Option<Range> {
    source.get(bytes.clone())?;
    Some(Range::new(
        offset_to_position(source, bytes.start)?,
        offset_to_position(source, bytes.end)?,
    ))
}

/// Convert inclusive parser scalar positions using their original file's text.
/// Byte offsets are deliberately unused: reindented includes can retain offsets
/// into preprocessed text. A range crossing source files cannot be converted.
pub(crate) fn location_to_utf16_range(source: &str, location: &Location) -> Option<Range> {
    if location.start.file != location.end.file {
        return None;
    }
    let start = scalar_offset(
        source,
        location.start.line,
        location.start.column.checked_sub(1)?,
    )?;
    let end = scalar_offset(source, location.end.line, location.end.column)?;
    byte_range_to_utf16_range(source, start..end)
}

fn scalar_offset(source: &str, line: u32, boundary: u32) -> Option<usize> {
    let target_line = line.checked_sub(1)? as usize;
    let boundary = boundary as usize;
    let mut start = 0;
    for (index, text) in source.split_inclusive('\n').enumerate() {
        if index == target_line {
            return text
                .char_indices()
                .map(|(offset, _)| offset)
                .nth(boundary)
                .or_else(|| (boundary == text.chars().count()).then_some(text.len()))
                .map(|offset| start + offset);
        }
        start += text.len();
    }
    (boundary == 0 && target_line == source.bytes().filter(|&byte| byte == b'\n').count())
        .then_some(source.len())
}

fn offset_to_position(source: &str, offset: usize) -> Option<Position> {
    let prefix = source.get(..offset)?;
    let line = prefix.bytes().filter(|&byte| byte == b'\n').count();
    let tail = prefix.rsplit('\n').next()?;
    let tail = tail.strip_suffix('\r').unwrap_or(tail);
    Some(Position::new(
        to_lsp_u32(line),
        to_lsp_u32(tail.encode_utf16().count()),
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::convert::position_to_offset;

    fn location(start: (u32, u32), end: (u32, u32)) -> Location {
        let mut location = Location::default();
        location.start = acdc_parser::Position::new(start.0, start.1);
        location.end = acdc_parser::Position::new(end.0, end.1);
        location.absolute_start = usize::MAX;
        location.absolute_end = usize::MAX;
        location
    }

    #[test]
    fn inclusive_scalar_endpoints_count_utf16_units() {
        let text = "é😀x\r\nΩ🚀";
        assert_eq!(
            location_to_utf16_range(text, &location((1, 2), (1, 2))),
            Some(Range::new(Position::new(0, 1), Position::new(0, 3)))
        );
        assert_eq!(
            location_to_utf16_range(text, &location((1, 2), (2, 2))),
            Some(Range::new(Position::new(0, 1), Position::new(1, 3)))
        );
        assert_eq!(
            location_to_utf16_range(text, &location((2, 1), (2, 2))),
            Some(Range::new(Position::new(1, 0), Position::new(1, 3)))
        );
    }

    #[test]
    fn byte_boundaries_round_trip_for_unicode_and_crlf() {
        let text = "😀é\r\n🚀中\n";
        for (offset, ch) in text.char_indices() {
            if ch == '\r' || ch == '\n' {
                continue;
            }
            let range = byte_range_to_utf16_range(text, offset..offset + ch.len_utf8());
            assert_eq!(
                range.and_then(|range| position_to_offset(text, range.start)),
                Some(offset)
            );
            assert_eq!(
                range.and_then(|range| position_to_offset(text, range.end)),
                Some(offset + ch.len_utf8())
            );
        }
        assert_eq!(
            byte_range_to_utf16_range(text, text.len()..text.len()),
            Some(Range::new(Position::new(2, 0), Position::new(2, 0)))
        );
        assert_eq!(byte_range_to_utf16_range("", 0..0), Some(Range::default()));
    }

    #[test]
    fn newline_endpoints_become_the_next_line_start() {
        assert_eq!(
            location_to_utf16_range("😀\nnext", &location((1, 1), (1, 2))),
            Some(Range::new(Position::new(0, 0), Position::new(1, 0)))
        );
        assert_eq!(
            location_to_utf16_range("😀\r\nnext", &location((1, 1), (1, 3))),
            Some(Range::new(Position::new(0, 0), Position::new(1, 0)))
        );
        assert_eq!(
            byte_range_to_utf16_range("😀\r\n", 0..5),
            Some(Range::new(Position::new(0, 0), Position::new(0, 2)))
        );
    }

    #[test]
    fn invalid_and_cross_file_ranges_have_no_fallback() {
        assert!(byte_range_to_utf16_range("😀", 1..4).is_none());
        assert!(byte_range_to_utf16_range("😀", 0..3).is_none());
        assert!(byte_range_to_utf16_range("😀", 0..5).is_none());
        assert!(location_to_utf16_range("x", &location((1, 1), (1, 8))).is_none());
        assert!(location_to_utf16_range("x", &location((9, 1), (9, 1))).is_none());
        assert!(location_to_utf16_range("x", &Location::default()).is_none());
        assert!(location_to_utf16_range("xy", &location((1, 3), (1, 1))).is_none());
        let mut loc = location((1, 1), (1, 1));
        loc.end.file = Some(Arc::new(vec!["other.adoc".into()]));
        assert!(location_to_utf16_range("x", &loc).is_none());
        loc.start.file = loc.end.file.clone();
        assert!(location_to_utf16_range("x", &loc).is_some());
    }
}
