use proptest::prelude::*;

use super::LineMap;
use crate::Position;

#[test]
fn test_line_map_single_line() {
    let input = "Hello, World!";
    let line_map = LineMap::new(input);

    assert_eq!(line_map.line_starts, vec![0]);

    // Start of input
    let pos = line_map.offset_to_position(0, input);
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 1);

    // Middle of line
    let pos = line_map.offset_to_position(7, input);
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 8);

    // End of line
    let pos = line_map.offset_to_position(12, input);
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 13);
}

#[test]
fn test_line_map_multiple_lines() {
    let input = "Line 1\nLine 2\nLine 3";
    let line_map = LineMap::new(input);

    assert_eq!(line_map.line_starts, vec![0, 7, 14]);

    // Start of first line
    let pos = line_map.offset_to_position(0, input);
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 1);

    // End of first line (before newline)
    let pos = line_map.offset_to_position(6, input);
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 7);

    // Start of second line
    let pos = line_map.offset_to_position(7, input);
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 1);

    // Middle of second line
    let pos = line_map.offset_to_position(10, input);
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 4);

    // Start of third line
    let pos = line_map.offset_to_position(14, input);
    assert_eq!(pos.line, 3);
    assert_eq!(pos.column, 1);
}

#[test]
fn test_line_map_empty_lines() {
    let input = "Line 1\n\nLine 3";
    let line_map = LineMap::new(input);

    assert_eq!(line_map.line_starts, vec![0, 7, 8]);

    // Start of empty line
    let pos = line_map.offset_to_position(7, input);
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 1);

    // Start of line after empty line
    let pos = line_map.offset_to_position(8, input);
    assert_eq!(pos.line, 3);
    assert_eq!(pos.column, 1);
}

#[test]
fn test_line_map_asciidoc_example() {
    let input = "= Document Title\nLorn_Kismet R. Lee <kismet@asciidoctor.org>\nv2.9, 01-09-2024: Fall incarnation\n:description: The document's description.\n:sectanchors:\n:url-repo: https://my-git-repo.com";
    let line_map = LineMap::new(input);

    // Title start (after "= ")
    let pos = line_map.offset_to_position(2, input);
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 3);

    // Author line start (17 = length of "= Document Title\n")
    let pos = line_map.offset_to_position(17, input);
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 1);

    // Revision line start (61 = 17 + 44, where 44 is length of author line + newline)
    let pos = line_map.offset_to_position(61, input);
    assert_eq!(pos.line, 3);
    assert_eq!(pos.column, 1);
}

#[test]
fn test_line_map_beyond_input() {
    let input = "Hello";
    let line_map = LineMap::new(input);

    // Beyond input: offset is clamped to input.len(), giving position after last character
    let pos = line_map.offset_to_position(100, input);
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 6); // After 5 characters, column is 6
}

#[test]
fn test_line_map_empty_input() {
    let input = "";
    let line_map = LineMap::new(input);

    assert_eq!(line_map.line_starts, vec![0]);

    let pos = line_map.offset_to_position(0, input);
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 1);
}

#[test]
fn test_line_map_utf8_content() {
    // "é" is 2 bytes in UTF-8 (0xC3 0xA9). Line 1 is non-ASCII, line 2 is ASCII.
    // Bytes: c(0) a(1) f(2) é(3,4) \n(5) b(6) a(7) r(8)
    let input = "caf\u{00e9}\nbar";
    let line_map = LineMap::new(input);

    // Slow path: non-ASCII line, column via `chars().count()`.
    // Offset 5 = byte just past "café" (the '\n'); binary_search Err(1).
    let pos = line_map.offset_to_position(5, input);
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 5); // 4 chars in "café" + 1

    // Fast path on the line *after* a non-ASCII line: line_is_ascii[1] is true.
    let pos = line_map.offset_to_position(8, input);
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 3);

    // Mid-char offset: offset 4 lands inside the 2-byte 'é' and must
    // round back to byte 3 before counting chars.
    let pos = line_map.offset_to_position(4, input);
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 4); // "caf" = 3 chars + 1

    // Exact match on a line start that follows a non-ASCII line
    // (binary_search Ok arm, guarding line-index arithmetic).
    let pos = line_map.offset_to_position(6, input);
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 1);
}

proptest! {
    #[test]
    fn cached_columns_match_full_scan_in_arbitrary_order(
        chars in prop::collection::vec(prop_oneof![any::<char>(), Just('\n'), Just('a'), Just(' ')], 0..100),
        offsets in prop::collection::vec(0usize..512, 0..120),
    ) {
        let input: String = chars.into_iter().collect();
        let map = LineMap::new(&input);
        for offset in offsets {
            let actual = map.offset_to_position(offset, &input);
            let mut boundary = offset.min(input.len());
            while !input.is_char_boundary(boundary) {
                boundary -= 1;
            }
            let prefix = &input[..boundary];
            let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
            let column = prefix.rsplit('\n').next().unwrap_or_default().chars().count() + 1;
            prop_assert_eq!(actual, Position::from_line_col(line, column));
        }
    }
}
