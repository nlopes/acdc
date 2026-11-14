use acdc_parser::{
    CompatibilityMode, DocumentAttributeAssignment, DocumentAttributeValue, Options,
    ast::{BlockKind, BlockRef, ContentRef, InlineKind, LocationRef, StructuralRef},
    parse,
};
use rstest::rstest;

type Error = Box<dyn std::error::Error>;

fn text(nodes: ContentRef<'_>) -> Result<String, Error> {
    nodes
        .inlines()
        .map(|node| {
            if let InlineKind::Plain(text) = node.kind() {
                Ok(text.text())
            } else if let InlineKind::Raw(text) = node.kind() {
                Ok(text.text())
            } else {
                Err(format!("unexpected text node: {node:?}").into())
            }
        })
        .collect()
}

// Fixture JSON omits header attribute values. Force a conditional pass too,
// so both preprocessing paths preserve the same declaration.
#[test]
fn attribute_continuation_preserves_literal_backslashes_and_following_source() -> Result<(), Error>
{
    for separator in [" ", "   ", "\t", " \t "] {
        for newline in ["\n", "\r\n"] {
            for force_preprocessing in [false, true] {
                let directives = if force_preprocessing {
                    format!("ifdef::value[]{newline}endif::[]{newline}")
                } else {
                    String::new()
                };
                let source = format!(
                    "= Attributes{newline}:value:{separator}\\{newline}{directives}{newline}é [{{value}}].{newline}"
                );
                let parsed = parse(&source, &Options::default())?;
                assert_eq!(
                    parsed
                        .document()
                        .attributes()
                        .get("value")
                        .and_then(DocumentAttributeValue::text),
                    Some("\\"),
                    "{source:?}"
                );
                let [BlockKind::Paragraph(paragraph)] = &*parsed
                    .document()
                    .blocks()
                    .map(BlockRef::kind)
                    .collect::<Vec<_>>()
                else {
                    return Err(format!(
                        "unexpected blocks: {:?}",
                        parsed.document().blocks().collect::<Vec<_>>()
                    )
                    .into());
                };
                assert_eq!(text(paragraph.content())?, "é [\\].");
                let expected_line = if force_preprocessing { 6 } else { 4 };
                assert_eq!(paragraph.location().start().line(), expected_line);
                assert_eq!(paragraph.location().start().column(), 1);
                assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
            }
        }
    }
    Ok(())
}

#[test]
fn attribute_continuation_preserves_a_literal_backslash_at_eof() -> Result<(), Error> {
    for separator in [" ", "   ", "\t", " \t "] {
        let source = format!(":value:{separator}\\");
        let parsed = parse(&source, &Options::default())?;
        assert_eq!(
            parsed
                .document()
                .attributes()
                .get("value")
                .and_then(DocumentAttributeValue::text),
            Some("\\")
        );
        assert!(parsed.document().blocks().next().is_none());
    }
    Ok(())
}

#[test]
fn attribute_continuation_preserves_caller_locks_and_does_not_swallow_body_text()
-> Result<(), Error> {
    let source = ":value: \\\nFollowing paragraph.\n\nValue [{value}].\n";
    for (options, expected) in [
        (Options::default(), "Value [\\]."),
        (
            Options::builder()
                .with_attribute("value", "caller")
                .build()?,
            "Value [caller].",
        ),
    ] {
        let parsed = parse(source, &options)?;
        let [BlockKind::Paragraph(first), BlockKind::Paragraph(second)] = &*parsed
            .document()
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("expected two paragraphs after the declaration".into());
        };
        assert_eq!(text(first.content())?, "Following paragraph.");
        assert_eq!(text(second.content())?, expected);
        assert_eq!(first.location().start().line(), 2);
        assert_eq!(second.location().start().line(), 4);
    }
    Ok(())
}

#[test]
fn attribute_continuation_keeps_colon_prefixed_lines_in_the_value() -> Result<(), Error> {
    let source = "= Attributes\n:value: First \\\n:other: Still value text\n\nAfter [{value}].\n";
    let parsed = parse(source, &Options::default())?;
    assert_eq!(
        parsed
            .document()
            .attributes()
            .get("value")
            .and_then(DocumentAttributeValue::text),
        Some("First :other: Still value text")
    );
    assert!(!parsed.document().attributes().contains("other"));
    let [BlockKind::Paragraph(paragraph)] = &*parsed
        .document()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected only the paragraph following the continued declaration".into());
    };
    assert_eq!(
        text(paragraph.content())?,
        "After [First :other: Still value text]."
    );
    assert_eq!(paragraph.location().start().line(), 5);
    Ok(())
}

fn options(mode: CompatibilityMode) -> Result<Options<'static>, Error> {
    Ok(Options::builder().with_compatibility_mode(mode).build()?)
}

/// `(start line, start column, end line, end column)`.
fn span(location: LocationRef<'_>) -> (u32, u32, u32, u32) {
    (
        location.start().line(),
        location.start().column(),
        location.end().line(),
        location.end().column(),
    )
}

/// An attribute entry block as `(name, value, span)`.
type Entry = (String, String, (u32, u32, u32, u32));

/// Each attribute entry block in the document, nested ones included.
fn entries(source: &str, options: &Options<'_>) -> Result<Vec<Entry>, Error> {
    let parsed = parse(source, options)?;
    StructuralRef::Document(parsed.document())
        .descendants()
        .filter_map(|node| {
            if let StructuralRef::Block(block) = node
                && let BlockKind::DocumentAttribute(entry) = block.kind()
            {
                Some((block, entry))
            } else {
                None
            }
        })
        .map(|(block, entry)| {
            let DocumentAttributeAssignment::Set(value) = entry.assignment() else {
                return Err(format!("unexpected unset: {}", entry.name()).into());
            };
            Ok((
                entry.name().to_owned(),
                value.text().unwrap_or_default().to_owned(),
                span(block.location()),
            ))
        })
        .collect()
}

// The preprocessor keeps the lines of a wrapped value as written, so the
// entry's location ends on its last source line.
#[rstest]
fn wrapped_entry_locations_end_on_the_last_line(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "= Title\n\n:soft: one \\\n  two\n\n:hard: one + \\\n  two\n\n:mixed: one \\\ntwo + \\\n three \\\nfour\n\n:open: one \\\n\nPara\n";
    assert_eq!(
        entries(source, &options(mode)?)?,
        [
            ("soft".to_owned(), "one two".to_owned(), (3, 1, 4, 5)),
            ("hard".to_owned(), "one + \\\ntwo".to_owned(), (6, 1, 7, 5)),
            (
                "mixed".to_owned(),
                "one two + \\\nthree four".to_owned(),
                (9, 1, 12, 4)
            ),
            ("open".to_owned(), "one ".to_owned(), (14, 1, 14, 12)),
        ]
    );
    Ok(())
}

#[rstest]
fn header_span_covers_a_wrapped_last_entry(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    for (source, expected) in [
        (":a: one \\\ntwo\n\nPara\n", (1, 1, 2, 3)),
        (
            "= Title\n:a: one \\\n  two \\\n  three\n\nPara\n",
            (1, 1, 4, 7),
        ),
    ] {
        let parsed = parse(source, &options(mode)?)?;
        let header = parsed.document().header_location().ok_or("no header")?;
        assert_eq!(span(header), expected, "{source:?}");
        assert_eq!(
            parsed
                .document()
                .attributes()
                .get("a")
                .and_then(DocumentAttributeValue::text),
            Some(if source.contains("three") {
                "one two three"
            } else {
                "one two"
            }),
            "{source:?}"
        );
    }
    Ok(())
}

// The preprocessor leaves table cells alone, so the grammar joins the value.
// Asciidoctor gives `one two` here too.
#[rstest]
fn wrapped_entry_in_an_asciidoc_cell_joins_its_lines(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "= Title\n\n|===\na|\n:cell: one \\\ntwo\n\nText\n|===\n";
    assert_eq!(
        entries(source, &options(mode)?)?,
        [("cell".to_owned(), "one two".to_owned(), (5, 1, 6, 3))]
    );
    Ok(())
}
