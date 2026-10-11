use std::{io::Cursor, path::PathBuf};

use acdc_parser::{
    Block, InlineNode, Options, ParseResult, SafeMode, Warning, WarningKind, parse,
    parse_from_reader,
};

type Error = Box<dyn std::error::Error>;

// Expected text was captured from Asciidoctor 2.0.26 HTML output.
#[rstest::rstest]
#[case("", "[{toc}] [{toc-position}] [auto] [{toc-class}]")]
#[case(":toc:\n", "[] [{toc-position}] [auto] [{toc-class}]")]
#[case(":toc: left\n", "[] [left] [auto] [toc2]")]
#[case(":toc: >\n", "[] [right] [auto] [toc2]")]
#[case(":toc: macro\n", "[] [content] [macro] [{toc-class}]")]
#[case(":toc: preamble\n", "[] [content] [preamble] [{toc-class}]")]
#[case(":toc: other\n", "[] [{toc-position}] [auto] [{toc-class}]")]
#[case(":toc2:\n", "[] [left] [auto] [toc2]")]
#[case(":toc: left\n:toc-position: right\n", "[] [right] [auto] [toc2]")]
#[case(
    ":toc: left\n:toc-placement: macro\n",
    "[] [content] [macro] [{toc-class}]"
)]
#[case(":toc: left\n:toc-class: custom\n", "[] [left] [auto] [custom]")]
#[case(
    ":toc-placement!:\n",
    "[{toc}] [{toc-position}] [{toc-placement}] [{toc-class}]"
)]
#[case(":toc: left\n:toc-placement!:\n", "[] [content] [macro] [{toc-class}]")]
fn toc_header_normalization(#[case] header: &str, #[case] expected: &str) -> Result<(), Error> {
    // The final period keeps the probe from being parsed as block metadata.
    let input = format!(
        "= T\n{header}\n[{{toc}}] [{{toc-position}}] [{{toc-placement}}] [{{toc-class}}].\n"
    );
    let parsed = parse(&input, &Options::default())?;
    assert_eq!(
        paragraphs(&parsed.document().blocks),
        [format!("{expected}.")]
    );
    assert!(!parsed.document().attributes.contains_key("toc2"));
    Ok(())
}

#[rstest::rstest]
#[case(false, "[] [left] [auto]", "[] [left] [auto]")]
#[case(true, "[] [right] [auto]", "[macro] [right] [auto]")]
fn toc_normalization_preserves_caller_precedence(
    #[case] soft: bool,
    #[case] first: &str,
    #[case] last: &str,
) -> Result<(), Error> {
    let builder = Options::builder();
    let options = if soft {
        builder.with_default_attribute("toc", "left")
    } else {
        builder.with_attribute("toc", "left")
    }
    .build()?;
    let parsed = parse(
        "= T\n:toc: right\n\nFirst: [{toc}] [{toc-position}] [{toc-placement}].\n\n:toc: macro\n\nLast: [{toc}] [{toc-position}] [{toc-placement}].\n",
        &options,
    )?;
    assert_eq!(
        paragraphs(&parsed.document().blocks),
        [format!("First: {first}."), format!("Last: {last}.")]
    );
    Ok(())
}

#[test]
fn toc_normalization_preserves_caller_unset() -> Result<(), Error> {
    let parsed = parse(
        "= T\n:toc: right\n\nFirst: [{toc}] [{toc-position}] [{toc-placement}].\n\n:toc: macro\n\nLast: [{toc}] [{toc-position}] [{toc-placement}].\n",
        &Options::builder().with_attribute("toc", ()).build()?,
    )?;
    assert_eq!(
        paragraphs(&parsed.document().blocks),
        [
            "First: [{toc}] [{toc-position}] [auto].",
            "Last: [{toc}] [{toc-position}] [auto]."
        ]
    );
    Ok(())
}

#[test]
fn toc_normalization_stops_at_header_boundary() -> Result<(), Error> {
    let parsed = parse(
        "= T\n:toc: left\n\nFirst.\n\n:toc: right\n\n[{toc}] [{toc-position}] [{toc-placement}] [{toc-class}].\n",
        &Options::default(),
    )?;
    assert_eq!(
        paragraphs(&parsed.document().blocks),
        ["First.", "[right] [left] [auto] [toc2]."]
    );
    Ok(())
}

#[rstest::rstest]
#[case(":toc2:\n\nBody.\n", 2)]
#[case(":toc2:\n:toc2:\n\nBody.\n\n:toc2:\n\nMore.\n", 2)]
#[case("\nBody.\n\n:toc2:\n\n:toc2:\n\nMore.\n", 5)]
fn toc2_settings_warn_once_at_first_document_entry(
    #[case] entries: &str,
    #[case] line: u32,
) -> Result<(), Error> {
    let parsed = parse(&format!("= T\n{entries}"), &Options::default())?;
    assert_eq!(toc2_warnings(&parsed).count(), 1);
    let warning = toc2_warnings(&parsed)
        .next()
        .ok_or("missing toc2 warning")?;
    let location = warning.source_location().ok_or("missing source location")?;
    assert_eq!(location.location.start.line, line);
    assert_eq!(location.location.start.column, 1);
    assert!(
        warning
            .advice()
            .is_some_and(|advice| advice.contains(":toc: left"))
    );
    assert!(parsed.source_recovery().is_none());
    Ok(())
}

#[test]
fn toc2_caller_settings_warn_once_on_each_parse() -> Result<(), Error> {
    for options in [
        Options::builder().with_attribute("toc2", true).build()?,
        Options::builder()
            .with_default_attribute("toc2", true)
            .build()?,
        Options::with_attributes([("toc2", true)])?,
    ] {
        for _ in 0..2 {
            let parsed = parse("= T\n:toc2:\n\nBody.\n\n:toc2:\n\nMore.\n", &options)?;
            assert_eq!(toc2_warnings(&parsed).count(), 1);
            let warning = toc2_warnings(&parsed)
                .next()
                .ok_or("missing toc2 warning")?;
            assert!(warning.source_location().is_none());
            assert_eq!(
                parsed
                    .document()
                    .attributes
                    .get("toc-position")
                    .and_then(|value| value.as_str()),
                Some("left"),
            );
        }
    }
    Ok(())
}

#[test]
fn toc2_reader_input_retains_source_location() -> Result<(), Error> {
    let parsed = parse_from_reader(
        &mut Cursor::new("= T\n:toc2:\n\nBody.\n"),
        &Options::default(),
    )?;
    let warning = toc2_warnings(&parsed)
        .next()
        .ok_or("missing toc2 warning")?;
    assert_eq!(
        warning
            .source_location()
            .ok_or("missing source location")?
            .location
            .start
            .line,
        2
    );
    Ok(())
}

#[rstest::rstest]
#[case(":toc2!:\n")]
#[case(":!toc2:\n")]
#[case(":toc: left\n")]
#[case(":toc: left\n:toc-class: toc2\n")]
#[case("\nifdef::missing-attribute[]\n:toc2:\nendif::[]\n")]
fn toc2_unsets_and_modern_toc_do_not_warn(#[case] entries: &str) -> Result<(), Error> {
    let parsed = parse(&format!("= T\n{entries}\nBody.\n"), &Options::default())?;
    assert_eq!(toc2_warnings(&parsed).count(), 0);
    Ok(())
}

#[rstest::rstest]
#[case(false, 0)]
#[case(true, 1)]
fn toc2_unset_preserves_caller_locks(
    #[case] soft: bool,
    #[case] expected: usize,
) -> Result<(), Error> {
    let options = if soft {
        Options::builder().with_default_attribute("toc2", ())
    } else {
        Options::builder().with_attribute("toc2", ())
    }
    .build()?;
    let parsed = parse("= T\n:toc2:\n\nBody.\n", &options)?;
    assert_eq!(toc2_warnings(&parsed).count(), expected);
    assert_eq!(parsed.document().attributes.contains_key("toc"), soft);
    Ok(())
}

#[rstest::rstest]
#[case("", 1)]
#[case("lines=3", 0)]
fn toc2_include_warnings_keep_original_source(
    #[case] selection: &str,
    #[case] expected: usize,
) -> Result<(), Error> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests/includes");
    let options = Options::builder()
        .with_safe_mode(SafeMode::Safe)
        .with_base_dir(&directory)
        .build()?;
    let parsed = parse(
        &format!("= T\ninclude::toc2.adoc[{selection}]\n\nBody.\n"),
        &options,
    )?;
    assert_eq!(toc2_warnings(&parsed).count(), expected);
    if let Some(warning) = toc2_warnings(&parsed).next() {
        let location = warning
            .source_location()
            .ok_or("missing include location")?;
        assert_eq!(location.file, Some(directory.join("toc2.adoc")));
        assert_eq!(location.location.start.line, 1);
        assert_eq!(location.location.start.column, 1);
    }
    Ok(())
}

fn toc2_warnings(parsed: &ParseResult) -> impl Iterator<Item = &Warning> {
    parsed
        .warnings()
        .iter()
        .filter(|warning| matches!(warning.kind, WarningKind::DeprecatedToc2Attribute))
}

fn paragraphs(blocks: &[Block<'_>]) -> Vec<String> {
    blocks
        .iter()
        .filter_map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return None;
            };
            Some(
                paragraph
                    .content
                    .iter()
                    .filter_map(|node| {
                        let InlineNode::PlainText(text) = node else {
                            return None;
                        };
                        Some(text.content)
                    })
                    .collect(),
            )
        })
        .collect()
}
