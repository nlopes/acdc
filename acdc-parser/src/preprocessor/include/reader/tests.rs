use super::*;
use crate::Preprocessor;
use proptest::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Chunks {
    bytes: Cursor<Vec<u8>>,
    size: usize,
}

impl Read for Chunks {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let count = out.len().min(self.size);
        self.bytes.read(out.get_mut(..count).unwrap_or_default())
    }
}

fn source(bytes: impl Into<Vec<u8>>, size: usize) -> IncludeSource {
    IncludeSource::from_reader(Chunks {
        bytes: Cursor::new(bytes.into()),
        size,
    })
}

fn compare_buffered(input: &str, selection: &ContentSelection, chunk: usize) -> Result<(), Error> {
    let normalized = Preprocessor::normalize(input);
    let lines = if matches!(selection, ContentSelection::Lines(_)) {
        input.lines().map(str::trim_end).collect::<Vec<_>>()
    } else {
        normalized.lines().collect::<Vec<_>>()
    };
    let mut issues = Vec::new();
    let indices = match selection {
        ContentSelection::All => (0..lines.len()).collect::<Vec<_>>(),
        ContentSelection::Lines(ranges) => {
            let options = crate::Options::default();
            let include = super::super::tests::parse_include(
                std::path::Path::new("/tmp"),
                "include::part.adoc[]",
                &options,
            )?;
            let mut indices = include
                .collect_line_range_indices(ranges, lines.len())
                .into_iter()
                .collect::<Vec<_>>();
            indices.sort_unstable();
            indices
        }
        ContentSelection::Tags(filters) => {
            tag::select_tagged_lines(&lines, filters, |issue| issues.push(issue))
        }
    };
    let selected = read(
        source(input.as_bytes(), chunk),
        None,
        "part.adoc",
        selection,
    )?;
    assert_eq!(
        selected.text,
        indices
            .iter()
            .filter_map(|&index| lines.get(index).copied())
            .collect::<Vec<_>>()
            .join("\n"),
        "{input:?}"
    );
    let offsets = super::super::Include::line_start_offsets(&lines);
    assert_eq!(
        selected
            .origins
            .iter()
            .map(|origin| (origin.line, origin.offset))
            .collect::<Vec<_>>(),
        indices
            .iter()
            .filter_map(|&index| offsets.get(index).map(|offset| (index + 1, *offset)))
            .collect::<Vec<_>>(),
        "{input:?}"
    );
    assert_eq!(selected.issues, issues, "{input:?}");
    Ok(())
}

proptest! {
    #[test]
    fn streaming_selection_matches_buffered_contract(
        parts in prop::collection::vec(prop::sample::select(vec!["a", "λ", " ", "\t", "\r", "\n", "\u{2003}", "// tag::x[]", "// end::x[]", "// tag::y[]", "// end::y[]", "notatag::x[]", "tag::[]", "tag::x[[]", "tag::x[]end::y[]"]), 0..50),
        chunk in 1..32_usize,
        start in 0..10_usize,
        end in -2..15_isize,
    ) {
        let input = parts.concat();
        compare_buffered(&input, &ContentSelection::All, chunk)?;
        compare_buffered(&input, &ContentSelection::Lines(vec![LinesRange::Range(start, end), LinesRange::Single(2)]), chunk)?;
        for names in [["x", "!y"], ["**", "!x"], ["*", "y"], ["!x", "!y"]] {
            let filters = names.iter().filter_map(|name| tag::Filter::parse(name)).collect();
            compare_buffered(&input, &ContentSelection::Tags(filters), chunk)?;
        }
    }
}

#[test]
fn selected_content_accepts_exact_limit_and_rejects_one_more_byte() -> TestResult {
    let accepted = read(
        IncludeSource::from_reader(io::repeat(b'a').take(u64::try_from(MAX_SELECTED_BYTES)?)),
        None,
        "part.adoc",
        &ContentSelection::All,
    )?;
    assert_eq!(accepted.text.len(), MAX_SELECTED_BYTES);
    // An endless nonblank line must fail before a newline or the end of the source.
    assert!(matches!(
        read(
            IncludeSource::from_reader(io::repeat(b'a')),
            None,
            "part.adoc",
            &ContentSelection::All
        ),
        Err(Error::IncludeSourceTooLarge(_))
    ));
    Ok(())
}

#[test]
fn selected_line_after_oversized_skipped_line_keeps_origin() -> TestResult {
    let skipped = MAX_SELECTED_BYTES * 2;
    let reader = io::repeat(b'x')
        .take(u64::try_from(skipped)?)
        .chain(Cursor::new(b"\nChosen.\n"));
    let selected = read(
        IncludeSource::from_reader(reader),
        None,
        "part.adoc",
        &ContentSelection::Lines(vec![LinesRange::Single(2)]),
    )?;
    assert_eq!(selected.text, "Chosen.");
    assert_eq!(
        selected
            .origins
            .first()
            .map(|origin| (origin.line, origin.offset)),
        Some((2, skipped + 1))
    );
    Ok(())
}

#[test]
fn invalid_large_range_does_not_displace_a_valid_small_selection() -> TestResult {
    let reader = io::repeat(b'x')
        .take(u64::try_from(MAX_SELECTED_BYTES + 1)?)
        .chain(Cursor::new(b"\nChosen."));
    let selected = read(
        IncludeSource::from_reader(reader),
        None,
        "part.adoc",
        &ContentSelection::Lines(vec![LinesRange::Range(1, 3), LinesRange::Single(2)]),
    )?;
    assert_eq!(selected.text, "Chosen.");
    Ok(())
}

#[test]
fn discarded_candidates_from_a_past_eof_range_do_not_count_toward_the_cap() -> TestResult {
    let line = format!("{}\n", "x".repeat(600_000));
    let input = format!("{}Chosen.", line.repeat(20));
    let selection = ContentSelection::Lines(vec![LinesRange::Range(1, 30), LinesRange::Single(21)]);
    let selected = read(source(input, 8192), None, "part.adoc", &selection)?;
    assert_eq!(selected.text, "Chosen.");
    Ok(())
}

#[test]
fn tag_marker_after_oversized_skipped_prefix_is_recognized() -> TestResult {
    let reader = io::repeat(b'x')
        .take(u64::try_from(MAX_SELECTED_BYTES + 1)?)
        .chain(Cursor::new(b" // tag::x[]\nChosen.\n// end::x[]\n"));
    let selection = ContentSelection::Tags(tag::Filter::parse("x").into_iter().collect());
    let selected = read(
        IncludeSource::from_reader(reader),
        None,
        "part.adoc",
        &selection,
    )?;
    assert_eq!(selected.text, "Chosen.");
    assert_eq!(selected.issues, []);
    Ok(())
}

#[test]
fn decoder_handles_bom_and_code_points_split_across_reads() -> TestResult {
    let input = "Skip.\r\nCafé 🐈\nEnd.";
    let mut utf16 = vec![0xff, 0xfe];
    for unit in input.encode_utf16() {
        utf16.extend(unit.to_le_bytes());
    }
    let mut utf8 = vec![0xef, 0xbb, 0xbf];
    utf8.extend(input.as_bytes());
    let selection = ContentSelection::Lines(vec![LinesRange::Single(2)]);
    for bytes in [utf8, utf16] {
        for chunk in 1..8 {
            let selected = read(source(bytes.clone(), chunk), None, "part.adoc", &selection)?;
            assert_eq!(selected.text, "Café 🐈");
            assert_eq!(
                selected
                    .origins
                    .first()
                    .map(|origin| (origin.line, origin.offset)),
                Some((2, 6))
            );
        }
    }
    Ok(())
}

#[test]
fn selected_limit_counts_decoded_utf8_and_newline_separators() -> TestResult {
    let half = MAX_SELECTED_BYTES / 2;
    let input = vec![b'a'; half]
        .into_iter()
        .chain(*b"\n")
        .chain(vec![b'b'; half])
        .collect::<Vec<_>>();
    assert!(matches!(
        read(
            source(input, 8192),
            None,
            "part.adoc",
            &ContentSelection::All
        ),
        Err(Error::IncludeSourceTooLarge(_))
    ));
    // Each Windows-1252 0xe9 byte expands to two UTF-8 bytes.
    assert!(matches!(
        read(
            IncludeSource::from_reader(io::repeat(0xe9).take(u64::try_from(half + 1)?)),
            Some("windows-1252"),
            "part.adoc",
            &ContentSelection::All
        ),
        Err(Error::IncludeSourceTooLarge(_))
    ));
    Ok(())
}

#[cfg(feature = "network")]
#[test]
fn http_transfer_limit_is_separate_from_selected_content() -> TestResult {
    let selection = ContentSelection::Lines(vec![LinesRange::Single(1)]);
    for limit in [8, MAX_SELECTED_BYTES] {
        let input = Cursor::new(b"Chosen.\n").chain(io::repeat(b'x'));
        let mut source = IncludeSource::from_reader(input);
        source.read_limit = Some(limit);
        let selected = read(source, None, "https://example.test/part.adoc", &selection)?;
        assert_eq!(selected.text, "Chosen.");
    }

    let input = io::repeat(b'x')
        .take(u64::try_from(MAX_SELECTED_BYTES)?)
        .chain(Cursor::new(b"\nChosen.\n"));
    let mut source = IncludeSource::from_reader(input);
    source.read_limit = Some(MAX_SELECTED_BYTES);
    assert!(matches!(
        read(
            source,
            None,
            "https://example.test/part.adoc",
            &ContentSelection::Lines(vec![LinesRange::Single(2)])
        ),
        Err(Error::HttpRequest(_))
    ));
    Ok(())
}

#[test]
fn indent_cannot_expand_selection_past_cap() {
    let lines = vec!["a"; 3000];
    assert!(super::super::Include::apply_indent(&lines, 4096).is_none());
}

#[test]
fn tag_metadata_is_bounded_even_when_no_text_is_selected() -> TestResult {
    let reader = Cursor::new(b"// tag::")
        .chain(io::repeat(b'x').take(u64::try_from(MAX_TAG_METADATA_BYTES + 1)?))
        .chain(Cursor::new(b"[]\n"));
    let selection = ContentSelection::Tags(tag::Filter::parse("absent").into_iter().collect());
    assert!(matches!(
        read(
            IncludeSource::from_reader(reader),
            None,
            "part.adoc",
            &selection
        ),
        Err(Error::IncludeSelectionTooComplex(_))
    ));
    Ok(())
}

/// Return an error if the reader requests bytes after the selection is complete.
struct FailAfter(Cursor<Vec<u8>>);

impl Read for FailAfter {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let count = self.0.read(out)?;
        if count == 0 {
            return Err(io::Error::other("read beyond the selected range"));
        }
        Ok(count)
    }
}

#[test]
fn finite_lines_stop_without_an_eof_probe() -> TestResult {
    for input in ["\n", "a\n", "Chosen.\r\n"] {
        let selected = read(
            IncludeSource::from_reader(FailAfter(Cursor::new(input.as_bytes().to_vec()))),
            None,
            "part.adoc",
            &ContentSelection::Lines(vec![LinesRange::Single(1)]),
        )?;
        assert_eq!(selected.text, input.trim_end());
        assert_eq!(selected.origins.len(), 1);
    }
    Ok(())
}

#[test]
fn finite_lines_ignore_malformed_bytes_after_the_last_requested_line() -> TestResult {
    for chunk in [1, 2, 3, 8192] {
        let selected = read(
            source(b"Chosen.\n\xff", chunk),
            None,
            "part.adoc",
            &ContentSelection::Lines(vec![LinesRange::Single(1)]),
        )?;
        assert_eq!(selected.text, "Chosen.");
        for input in [b"\xff\nChosen.\n".as_slice(), b"Chosen.\xff\n".as_slice()] {
            assert!(matches!(
                read(
                    source(input, chunk),
                    None,
                    "part.adoc",
                    &ContentSelection::Lines(vec![LinesRange::Single(1)])
                ),
                Err(Error::UnrecognizedEncodingInFile(_))
            ));
        }
    }
    Ok(())
}

#[test]
fn full_open_ended_and_tag_selections_validate_through_eof() {
    let filters = tag::Filter::parse("sample").into_iter().collect();
    for selection in [
        ContentSelection::All,
        ContentSelection::Lines(vec![LinesRange::Range(1, -1)]),
        ContentSelection::Tags(filters),
    ] {
        assert!(matches!(
            read(
                source(b"// tag::sample[]\nChosen.\n// end::sample[]\n\xff", 8192),
                None,
                "part.adoc",
                &selection
            ),
            Err(Error::UnrecognizedEncodingInFile(_))
        ));
    }
}

#[test]
fn finite_lines_use_the_highest_valid_requested_line() -> TestResult {
    let input = "Skip.\nTwo.\nThree.\nFour.\n";
    let selection = ContentSelection::Lines(vec![
        LinesRange::Single(4),
        LinesRange::Range(2, 3),
        LinesRange::Single(2),
        LinesRange::Range(0, -1),
        LinesRange::Range(9, 8),
    ]);
    let selected = read(
        IncludeSource::from_reader(FailAfter(Cursor::new(input.as_bytes().to_vec()))),
        None,
        "part.adoc",
        &selection,
    )?;
    assert_eq!(selected.text, "Two.\nThree.\nFour.");
    assert_eq!(
        selected
            .origins
            .iter()
            .map(|origin| origin.line)
            .collect::<Vec<_>>(),
        [2, 3, 4]
    );
    Ok(())
}

#[test]
fn line_normalization_and_locations_do_not_depend_on_the_unused_tail() -> TestResult {
    for tail in ["", "After.\n", "After. \r\n"] {
        let input = format!("Skip.\u{2003}\nChosen.\u{2003}\n{tail}");
        let selected = read(
            source(input, 8192),
            None,
            "part.adoc",
            &ContentSelection::Lines(vec![LinesRange::Single(2)]),
        )?;
        assert_eq!(selected.text, "Chosen.");
        assert_eq!(
            selected
                .origins
                .first()
                .map(|origin| (origin.line, origin.offset)),
            Some((2, 6))
        );
    }
    let selected = read(
        source("Skip.\n\n", 8192),
        None,
        "part.adoc",
        &ContentSelection::Lines(vec![LinesRange::Single(2)]),
    )?;
    assert_eq!(selected.text, "");
    assert_eq!(selected.origins.first().map(|origin| origin.line), Some(2));
    Ok(())
}

#[test]
fn utf16_selection_ignores_a_truncated_code_unit_in_the_unused_tail() -> TestResult {
    let input = format!("{}\n", "界".repeat(7000));
    for little_endian in [false, true] {
        let mut bytes = if little_endian {
            vec![0xff, 0xfe]
        } else {
            vec![0xfe, 0xff]
        };
        for unit in input.encode_utf16() {
            bytes.extend(if little_endian {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        }
        bytes.push(0xff);
        for chunk in [1, 3, 8192] {
            let selected = read(
                source(bytes.clone(), chunk),
                None,
                "part.adoc",
                &ContentSelection::Lines(vec![LinesRange::Single(1)]),
            )?;
            assert_eq!(selected.text, input.trim_end());
        }
    }
    Ok(())
}

#[test]
fn empty_line_selection_does_not_read_the_source() -> TestResult {
    let selected = read(
        IncludeSource::from_reader(FailAfter(Cursor::new(Vec::new()))),
        None,
        "part.adoc",
        &ContentSelection::Lines(vec![LinesRange::Single(0), LinesRange::Range(2, 1)]),
    )?;
    assert_eq!(selected.text, "");
    assert_eq!(selected.origins.len(), 0);
    Ok(())
}
