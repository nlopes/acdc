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
    byte_range_to_utf16_range(
        source,
        location_bytes(location, |line, boundary| {
            scalar_offset(source, line, boundary)
        })?,
    )
}

fn location_bytes(
    location: &Location,
    offset: impl Fn(u32, u32) -> Option<usize>,
) -> Option<std::ops::Range<usize>> {
    if location.start.file != location.end.file {
        return None;
    }
    let start = offset(location.start.line, location.start.column.checked_sub(1)?)?;
    let end = offset(location.end.line, location.end.column)?;
    Some(start..end)
}

/// Original-source line offsets for converting many locations without rescanning the document.
pub(crate) struct LineIndex<'a> {
    source: &'a str,
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        Self {
            source,
            starts: std::iter::once(0)
                .chain(source.match_indices('\n').map(|(offset, _)| offset + 1))
                .collect(),
        }
    }

    pub(crate) fn range(&self, location: &Location) -> Option<Range> {
        let bytes = location_bytes(location, |line, boundary| {
            let index = line.checked_sub(1)?;
            let start = self.starts.get(index as usize)?;
            scalar_boundary(self.line(index)?, boundary as usize).map(|offset| start + offset)
        })?;
        self.source.get(bytes.clone())?;
        Some(Range::new(
            self.position(bytes.start)?,
            self.position(bytes.end)?,
        ))
    }

    pub(crate) fn line(&self, index: u32) -> Option<&'a str> {
        let index = index as usize;
        let start = *self.starts.get(index)?;
        let end = self
            .starts
            .get(index + 1)
            .copied()
            .unwrap_or(self.source.len());
        self.source.get(start..end)
    }

    pub(crate) fn line_width(&self, index: u32) -> Option<u32> {
        let line = self.line(index)?;
        let line = line.strip_suffix('\n').unwrap_or(line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        Some(to_lsp_u32(line.encode_utf16().count()))
    }

    fn position(&self, offset: usize) -> Option<Position> {
        let line = self
            .starts
            .partition_point(|&start| start <= offset)
            .checked_sub(1)?;
        let prefix = self.source.get(*self.starts.get(line)?..offset)?;
        Some(position_from_prefix(line, prefix))
    }
}

/// Convert a UTF-16 cursor to an original-source scalar position.
/// Reject positions inside a surrogate pair or outside the source.
pub(crate) fn position_to_scalar_position(
    source: &str,
    position: Position,
) -> Option<acdc_parser::Position> {
    let offset = super::position_to_offset(source, position)?;
    let prefix = source.get(..offset)?.rsplit('\n').next()?;
    Some(acdc_parser::Position::new(
        position.line.checked_add(1)?,
        u32::try_from(prefix.chars().count()).ok()?.checked_add(1)?,
    ))
}

fn scalar_offset(source: &str, line: u32, boundary: u32) -> Option<usize> {
    let target_line = line.checked_sub(1)? as usize;
    let boundary = boundary as usize;
    let mut start = 0;
    for (index, text) in source.split_inclusive('\n').enumerate() {
        if index == target_line {
            return scalar_boundary(text, boundary).map(|offset| start + offset);
        }
        start += text.len();
    }
    (boundary == 0 && target_line == source.bytes().filter(|&byte| byte == b'\n').count())
        .then_some(source.len())
}

fn scalar_boundary(text: &str, boundary: usize) -> Option<usize> {
    text.char_indices()
        .map(|(offset, _)| offset)
        .nth(boundary)
        .or_else(|| (boundary == text.chars().count()).then_some(text.len()))
}

fn offset_to_position(source: &str, offset: usize) -> Option<Position> {
    let prefix = source.get(..offset)?;
    let line = prefix.bytes().filter(|&byte| byte == b'\n').count();
    Some(position_from_prefix(line, prefix.rsplit('\n').next()?))
}

fn position_from_prefix(line: usize, prefix: &str) -> Position {
    let prefix = prefix.strip_suffix('\r').unwrap_or(prefix);
    Position::new(to_lsp_u32(line), to_lsp_u32(prefix.encode_utf16().count()))
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
    fn utf16_cursors_map_to_scalar_positions_without_byte_offsets() {
        let text = "😀x\r\né😀z\n";
        for (wire, scalar) in [
            (Position::new(0, 0), acdc_parser::Position::new(1, 1)),
            (Position::new(0, 2), acdc_parser::Position::new(1, 2)),
            (Position::new(1, 3), acdc_parser::Position::new(2, 3)),
            (Position::new(2, 0), acdc_parser::Position::new(3, 1)),
        ] {
            assert_eq!(position_to_scalar_position(text, wire), Some(scalar));
        }
        for invalid in [
            Position::new(0, 1),
            Position::new(1, 2),
            Position::new(1, 5),
            Position::new(3, 0),
        ] {
            assert_eq!(position_to_scalar_position(text, invalid), None);
        }
        assert_eq!(
            position_to_scalar_position("", Position::new(0, 0)),
            Some(acdc_parser::Position::new(1, 1))
        );
    }

    #[test]
    fn scalar_cursor_matching_ignores_byte_offsets_and_rejects_other_files() {
        let mut span = location((1, 2), (1, 3));
        assert!(crate::convert::position_in_location(
            &acdc_parser::Position::new(1, 2),
            &span
        ));
        assert!(crate::convert::position_in_location(
            &acdc_parser::Position::new(1, 3),
            &span
        ));
        assert!(!crate::convert::position_in_location(
            &acdc_parser::Position::new(1, 4),
            &span
        ));
        span.end.file = Some(Arc::new(vec!["included.adoc".into()]));
        assert!(!crate::convert::position_in_location(
            &acdc_parser::Position::new(1, 2),
            &span
        ));
        span.start.file = span.end.file.clone();
        assert!(!crate::convert::position_in_location(
            &acdc_parser::Position::new(1, 2),
            &span
        ));
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

    #[test]
    fn indexed_ranges_match_single_conversions_at_line_boundaries() {
        for text in ["", "é😀", "é😀\r\nΩ🚀\n", "x\n\n"] {
            let source = LineIndex::new(text);
            for start_line in 0..5 {
                for end_line in 0..5 {
                    for start_column in 0..7 {
                        for end_column in 0..7 {
                            let span = location((start_line, start_column), (end_line, end_column));
                            assert_eq!(
                                source.range(&span),
                                location_to_utf16_range(text, &span),
                                "{text:?}: {span:?}"
                            );
                        }
                    }
                }
            }
        }
        let mut span = location((1, 1), (1, 1));
        span.end.file = Some(Arc::new(vec!["other.adoc".into()]));
        assert!(LineIndex::new("x").range(&span).is_none());
    }
}
