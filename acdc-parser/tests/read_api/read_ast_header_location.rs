//! The document header span, with or without a document title.

use acdc_parser::{
    CompatibilityMode, Document, Header, Location, Options, Position, SafeMode, Title,
    ast::{DocumentRef, LocationRef},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

fn options(mode: CompatibilityMode) -> Result<Options<'static>, Error> {
    Ok(Options::builder().with_compatibility_mode(mode).build()?)
}

/// `(start line, start column, end line, end column)` of a span.
fn lines(span: LocationRef<'_>) -> (u32, u32, u32, u32) {
    (
        span.start().line(),
        span.start().column(),
        span.end().line(),
        span.end().column(),
    )
}

fn header_lines(
    source: &str,
    mode: CompatibilityMode,
) -> Result<Option<(u32, u32, u32, u32)>, Error> {
    let parsed = parse(source, &options(mode)?)?;
    Ok(parsed.document().header_location().map(lines))
}

#[rstest::rstest]
fn titled_header_span_is_the_header_location(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        ":a: 1\n= Title\nJane Doe\n:b: 2\n\nBody.\n",
        &options(mode)?,
    )?;
    let document = parsed.document();
    let header = document.header().ok_or("header")?;
    assert_eq!(document.header_location(), Some(header.location()));
    assert_eq!(lines(header.location()), (1, 1, 4, 5));
    Ok(())
}

#[rstest::rstest]
#[case::contiguous_entries(":a: 1\n:b: 2\n\nPara\n", Some((1, 1, 2, 5)))]
#[case::interior_comment_line(":a: 1\n// note\n:b: 2\n\nPara\n", Some((1, 1, 3, 5)))]
// The preprocessor drops a line comment that follows another line, as for a
// titled header, so the span ends with the last attribute entry.
#[case::trailing_comment_line(":a: 1\n// note\n\nPara\n", Some((1, 1, 1, 5)))]
#[case::trailing_comment_block(":a: 1\n////\nnote\n////\n\nPara\n", Some((1, 1, 4, 4)))]
#[case::leading_comment_is_not_part_of_the_header("// note\n:a: 1\n\nPara\n", Some((2, 1, 2, 5)))]
#[case::leading_empty_lines("\n\n:a: 1\n\nPara\n", Some((3, 1, 3, 5)))]
#[case::no_header("Para\n", None)]
#[case::only_comments("// note\n\nPara\n", None)]
#[case::empty_document("", None)]
#[case::attribute_entry_after_body("Para\n\n:a: 1\n", None)]
fn untitled_header_span_matches_in_both_modes(
    #[case] source: &str,
    #[case] expected: Option<(u32, u32, u32, u32)>,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(source, &options(mode)?)?;
    assert!(parsed.document().header().is_none());
    assert_eq!(parsed.document().header_location().map(lines), expected);
    Ok(())
}

#[test]
fn untitled_header_span_with_empty_lines_follows_the_mode() -> Result<(), Error> {
    let source = ":a: 1\n\n// note\n\n:b: 2\n\nPara\n";
    assert_eq!(
        header_lines(source, CompatibilityMode::Asciidoctor)?,
        Some((1, 1, 5, 5))
    );
    assert_eq!(
        header_lines(source, CompatibilityMode::Draft)?,
        Some((1, 1, 1, 5))
    );
    Ok(())
}

#[test]
fn draft_header_before_an_empty_line_and_title_has_no_title() -> Result<(), Error> {
    let source = ":a: 1\n\n= Title\n\nBody.\n";
    let parsed = parse(source, &options(CompatibilityMode::Draft)?)?;
    assert!(parsed.document().header().is_none());
    assert_eq!(
        parsed.document().header_location().map(lines),
        Some((1, 1, 1, 5))
    );
    let parsed = parse(source, &options(CompatibilityMode::Asciidoctor)?)?;
    let header = parsed.document().header().ok_or("header")?;
    assert_eq!(lines(header.location()), (1, 1, 3, 7));
    assert_eq!(parsed.document().header_location(), Some(header.location()));
    Ok(())
}

#[rstest::rstest]
fn untitled_header_span_uses_byte_offsets(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":café: é\n:b: 2\n\nPara\n";
    let parsed = parse(source, &options(mode)?)?;
    let span = parsed.document().header_location().ok_or("span")?;
    assert_eq!(span.absolute_start(), 0);
    assert_eq!(span.absolute_end(), source.find("2\n").ok_or("end")?);
    assert_eq!(lines(span), (1, 1, 2, 5));
    Ok(())
}

#[rstest::rstest]
fn untitled_header_span_covers_preprocessor_directives(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        ":a: 1\nifdef::a[]\n:b: 2\nendif::[]\nifdef::missing[]\n:c: 3\nendif::[]\n:d: 4\n\nPara\n",
        &options(mode)?,
    )?;
    let document = parsed.document();
    assert!(document.attributes().is_explicit("b"));
    assert!(!document.attributes().is_explicit("c"));
    assert_eq!(document.header_location().map(lines), Some((1, 1, 8, 5)));
    Ok(())
}

#[rstest::rstest]
fn untitled_header_span_ends_in_an_included_file(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let main = directory.path().join("main.adoc");
    std::fs::write(&main, ":a: 1\ninclude::attributes.adoc[]\n\nPara\n")?;
    std::fs::write(directory.path().join("attributes.adoc"), ":b: 2\n:c: 3\n")?;
    let parsed = parse_file(
        &main,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let document = parsed.document();
    assert!(document.attributes().is_explicit("c"));
    let span = document.header_location().ok_or("span")?;
    assert_eq!(lines(span), (1, 1, 2, 5));
    assert!(span.start().include_chain().is_none());
    assert_eq!(
        span.end().include_chain().map(Iterator::collect::<Vec<_>>),
        Some(vec!["attributes.adoc"])
    );
    Ok(())
}

#[test]
fn copied_documents_keep_the_untitled_header_span() -> Result<(), Error> {
    let parsed = parse(":a: 1\n\nPara\n", &Options::default())?;
    let mut copy = Document::from(parsed.document());
    assert_eq!(
        DocumentRef::from(&copy).header_location().map(lines),
        Some((1, 1, 1, 5))
    );
    // A supplied header takes over the span.
    let mut location = Location::default();
    location.absolute_end = 4;
    location.start = Position::new(1, 1);
    location.end = Position::new(1, 5);
    copy.set_header(Some(Header::new(Title::default(), location.clone())));
    assert_eq!(
        DocumentRef::from(&copy).header_location(),
        Some(LocationRef::from(&location))
    );
    assert!(
        DocumentRef::from(&Document::default())
            .header_location()
            .is_none()
    );
    Ok(())
}
