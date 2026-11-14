//! An attribute entry needs whitespace or the end of the line after `:name:`.
//!
//! This follows asciidoctor's `AttributeEntryRx`: a line such as `:foo:bar` is
//! ordinary text in every position, and it never sets `foo`. An unset entry
//! ignores any text after the separator; unlike asciidoctor, acdc warns.

use acdc_parser::{
    CompatibilityMode, DocumentAttributeValue, Options, ParseResult, WarningKind,
    ast::{BlockKind, BlockRef, ContentRef, DocumentRef, HeaderRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn options(mode: CompatibilityMode) -> Result<Options<'static>, Error> {
    Ok(Options::builder().with_compatibility_mode(mode).build()?)
}

/// The ignored-text warnings for unset entries, as `(name, text, line)`.
fn unset_value_warnings(parsed: &ParseResult) -> Vec<(String, String, Option<u32>)> {
    parsed
        .warnings()
        .iter()
        .filter_map(|warning| {
            if let WarningKind::UnsetAttributeValue { name, value } = &warning.kind {
                let line = warning
                    .source_location()
                    .map(|source| source.location.start.line);
                Some((name.clone(), value.clone(), line))
            } else {
                None
            }
        })
        .collect()
}

fn text(content: ContentRef<'_>) -> Result<String, Error> {
    let mut result = String::new();
    for inline in content.inlines() {
        let InlineKind::Plain(node) = inline.kind() else {
            return Err(format!("unexpected inline: {:?}", inline.kind()).into());
        };
        result.push_str(node.text());
    }
    Ok(result)
}

/// The body in source order: `attribute <name>` for each attribute entry and
/// the text of each paragraph.
fn body(document: DocumentRef<'_>) -> Result<Vec<String>, Error> {
    document
        .blocks()
        .map(|block| {
            let kind = block.kind();
            if let BlockKind::DocumentAttribute(attribute) = kind {
                Ok(format!("attribute {}", attribute.name()))
            } else if let BlockKind::Paragraph(paragraph) = kind {
                text(paragraph.content())
            } else {
                Err(format!("unexpected block: {kind:?}").into())
            }
        })
        .collect()
}

#[rstest::rstest]
fn name_text_line_in_body_is_a_paragraph(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Title\n\nPara\n\n:foo:bar\n\nfoo is {foo}\n",
        &options(mode)?,
    )?;
    let document = parsed.document();
    assert_eq!(body(document)?, ["Para", ":foo:bar", "foo is {foo}"]);
    assert!(!document.attributes().contains("foo"));
    Ok(())
}

#[rstest::rstest]
fn name_text_line_does_not_start_an_untitled_header(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(":foo:bar\n:baz: qux\n", &options(mode)?)?;
    let document = parsed.document();
    assert!(document.header().is_none());
    assert!(document.header_location().is_none());
    assert_eq!(body(document)?, [":foo:bar\n:baz: qux"]);
    assert!(!document.attributes().contains("foo"));
    assert!(!document.attributes().contains("baz"));
    Ok(())
}

#[rstest::rstest]
fn name_text_line_after_title_is_the_author_line(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse("= Title\n:foo:bar\n\nBody\n", &options(mode)?)?;
    let document = parsed.document();
    assert_eq!(
        document
            .attributes()
            .get("author")
            .and_then(DocumentAttributeValue::text),
        Some(":foo:bar")
    );
    assert_eq!(
        document
            .header()
            .ok_or("header")?
            .authors()
            .next()
            .ok_or("author")?
            .first_name(),
        ":foo:bar"
    );
    assert!(!document.attributes().contains("foo"));
    assert_eq!(body(document)?, ["Body"]);
    Ok(())
}

#[rstest::rstest]
fn name_text_line_after_author_ends_the_header(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Title\nJane Doe\n:a: 1\n:foo:bar\n:b: 2\n\nBody\n",
        &options(mode)?,
    )?;
    let document = parsed.document();
    let header = document.header().ok_or("header")?;
    assert_eq!(
        header.authors().next().ok_or("author")?.first_name(),
        "Jane"
    );
    assert_eq!(document.header_location().ok_or("span")?.end().line(), 3);
    assert_eq!(
        document.header().map(HeaderRef::location),
        document.header_location()
    );
    assert!(document.attributes().contains("a"));
    for name in ["foo", "b"] {
        assert!(!document.attributes().contains(name), "{name}");
    }
    assert_eq!(body(document)?, [":foo:bar\n:b: 2", "Body"]);
    Ok(())
}

#[rstest::rstest]
fn name_text_line_after_block_metadata_is_paragraph_text(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse("= Title\n\n[.lead]\n:foo:bar\nPara\n", &options(mode)?)?;
    let document = parsed.document();
    let [BlockKind::Paragraph(paragraph)] =
        &*document.blocks().map(BlockRef::kind).collect::<Vec<_>>()
    else {
        return Err("expected one paragraph".into());
    };
    assert!(paragraph.roles().contains("lead"));
    assert_eq!(text(paragraph.content())?, ":foo:bar\nPara");
    Ok(())
}

#[rstest::rstest]
fn name_text_line_does_not_define_attributes_for_conditionals(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Title\n\n:foo:bar\n\nifdef::foo[]\nDefined.\nendif::[]\n",
        &options(mode)?,
    )?;
    assert_eq!(body(parsed.document())?, [":foo:bar"]);
    Ok(())
}

#[rstest::rstest]
fn name_text_line_ending_in_backslash_is_not_continued(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Title\n\n:foo:bar \\\nnext\n\n:ok: first \\\nsecond\n\n{ok}\n",
        &options(mode)?,
    )?;
    let document = parsed.document();
    assert_eq!(
        body(document)?,
        [":foo:bar \\\nnext", "attribute ok", "first second"]
    );
    Ok(())
}

const CONDITIONALS: &str =
    "ifdef::foo[]\nfoo is set\nendif::[]\nifndef::foo[]\nfoo is unset\nendif::[]\n";

/// An unset entry ignores the text after the separator, in conditionals as
/// well as in attribute references, and warns once about the ignored text.
#[rstest::rstest]
fn unset_entry_with_text_unsets_and_warns(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(
        (":!foo: ignored", "ignored"),
        (":foo!: ignored", "ignored"),
        (":!foo:\tignored", "ignored"),
        (":!foo: first \\\nsecond", "first second")
    )]
    case: (&str, &str),
    #[values(true, false)] in_header: bool,
) -> Result<(), Error> {
    let (entry, ignored) = case;
    let gap = if in_header { "" } else { "\n" };
    let source = format!("= Title\n:foo: yes\n{gap}{entry}\n\n{CONDITIONALS}\n{{foo}}\n");
    let parsed = parse(&source, &options(mode)?)?;
    let document = parsed.document();
    let expected: &[&str] = if in_header {
        &["foo is unset", "{foo}"]
    } else {
        &["attribute foo", "foo is unset", "{foo}"]
    };
    assert_eq!(body(document)?, expected, "{source:?}");
    assert_eq!(
        document.attributes().get("foo").is_none(),
        in_header,
        "{source:?}"
    );
    let line = if in_header { 3 } else { 4 };
    assert_eq!(
        unset_value_warnings(&parsed),
        [("foo".to_owned(), ignored.to_owned(), Some(line))],
        "{source:?}"
    );
    Ok(())
}

/// The warning covers every place an entry applies: an untitled header, block
/// metadata, and a caller-locked attribute whose unset is rejected.
#[rstest::rstest]
fn unset_entry_with_text_warns_wherever_it_applies(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(":!foo: ignored\n\nBody\n", &options(mode)?)?;
    assert_eq!(
        unset_value_warnings(&parsed),
        [("foo".to_owned(), "ignored".to_owned(), Some(1))]
    );
    let parsed = parse("= Title\n\n[.lead]\n:bar!: also\nPara\n", &options(mode)?)?;
    assert_eq!(
        unset_value_warnings(&parsed),
        [("bar".to_owned(), "also".to_owned(), Some(4))]
    );
    let locked = Options::builder()
        .with_compatibility_mode(mode)
        .with_attribute("foo", "locked")
        .build()?;
    let parsed = parse("= Title\n:!foo: ignored\n\n{foo}\n", &locked)?;
    assert_eq!(body(parsed.document())?, ["locked"]);
    assert_eq!(
        unset_value_warnings(&parsed),
        [("foo".to_owned(), "ignored".to_owned(), Some(2))]
    );
    Ok(())
}

#[rstest::rstest]
fn unset_entry_without_separator_is_text(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(":!foo:ignored", ":foo!:ignored")] line: &str,
) -> Result<(), Error> {
    let source = format!("= Title\n:foo: yes\n\n{line}\n\n{CONDITIONALS}");
    let parsed = parse(&source, &options(mode)?)?;
    assert_eq!(body(parsed.document())?, [line, "foo is set"], "{source:?}");
    assert_eq!(unset_value_warnings(&parsed), [], "{source:?}");
    Ok(())
}

#[rstest::rstest]
fn entries_with_separator_or_line_end_still_apply(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(
        (":foo:", Some("")),
        (":foo: bar", Some("bar")),
        (":foo:\tbar", Some("bar")),
        (":!foo:", None),
        (":foo!:", None)
    )]
    case: (&str, Option<&str>),
) -> Result<(), Error> {
    let (line, value) = case;
    let options = options(mode)?;
    for source in [
        format!("= Title\n{line}\n\nBody\n"),
        format!(":a: 1\n{line}\n\nBody\n"),
    ] {
        let parsed = parse(&source, &options)?;
        assert_eq!(parsed.warnings(), [], "{source:?}");
        let attributes = parsed.document().attributes();
        assert!(attributes.is_explicit("foo"), "{source:?}");
        let actual = attributes.get("foo");
        match value {
            Some("") => assert!(
                actual.is_some_and(DocumentAttributeValue::is_presence),
                "{source:?}"
            ),
            Some(text) => assert_eq!(
                actual.and_then(DocumentAttributeValue::text),
                Some(text),
                "{source:?}"
            ),
            None => assert!(actual.is_none(), "{source:?}"),
        }
    }
    // In the body, at the end of the input, and before more text.
    for source in [
        format!("= Title\n\n{line}"),
        format!("= Title\n\n{line}\n"),
        format!("= Title\n\n{line}\nPara\n"),
    ] {
        let parsed = parse(&source, &options)?;
        assert_eq!(parsed.warnings(), [], "{source:?}");
        let blocks = body(parsed.document())?;
        assert_eq!(
            blocks.first().map(String::as_str),
            Some("attribute foo"),
            "{source:?}"
        );
    }
    Ok(())
}
