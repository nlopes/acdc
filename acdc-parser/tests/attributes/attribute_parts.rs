use acdc_parser::{
    CompatibilityMode, DocumentAttributeValue, DocumentAttributes, Options, Substitution,
    ast::{BlockKind, InlineKind, TextPartRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn read_part(part: TextPartRef<'_>) -> (&str, Vec<Substitution>) {
    (part.text(), part.substitutions().collect())
}

fn spans<'a>(value: &'a DocumentAttributeValue<'_>) -> Vec<(&'a str, usize)> {
    value
        .parts()
        .map(|part| {
            let substitutions = part.substitutions().collect::<Vec<_>>();
            assert!(
                substitutions
                    .iter()
                    .all(|stage| *stage == Substitution::SpecialChars)
            );
            (part.text(), substitutions.len())
        })
        .collect()
}

#[rstest::rstest]
fn attribute_parts_preserves_definition_order_and_aliases(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        ":raw: pass:[<raw> &]\n:ordinary: <value> &\n:ac: pass:a,c[{raw}]\n:ca: pass:c,a[{raw}]\n:double: pass:a,c[{ordinary}]\n:alias: before {ac} / {ca} after\n:raw: Late\n\nBody.",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let attributes = parsed.document().attributes();
    for (name, expected) in [
        ("ordinary", vec![("<value> &", 1)]),
        ("ac", vec![("<raw> &", 1)]),
        ("ca", vec![("<raw> &", 0)]),
        ("double", vec![("<value> &", 2)]),
        (
            "alias",
            vec![
                ("before ", 1),
                ("<raw> &", 1),
                (" / ", 1),
                ("<raw> &", 0),
                (" after", 1),
            ],
        ),
    ] {
        let value = attributes.get(name).ok_or("missing attribute")?;
        assert_eq!(spans(value), expected, "{name}");
        assert_eq!(
            value.text(),
            Some(
                expected
                    .iter()
                    .map(|(text, _)| *text)
                    .collect::<String>()
                    .as_str()
            )
        );
    }
    Ok(())
}

#[rstest::rstest]
fn attribute_parts_survives_snapshot_reuse(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    let parsed = parse(
        ":label: pass:c[Literal &amp; and &#169;]\n\n{label}",
        &options,
    )?;
    let snapshot = DocumentAttributes::from(parsed.document().attributes()).to_static();
    let value = snapshot.get("label").ok_or("missing label")?;
    assert_eq!(value.text(), Some("Literal &amp; and &#169;"));
    assert_eq!(spans(value), [("Literal &amp; and &#169;", 1)]);
    assert_eq!(spans(&value.clone().into_static()), spans(value));
    let reparsed = parse("{label}", &options.with_document_attributes(snapshot))?;
    for document in [parsed.document(), reparsed.document()] {
        let references = document
            .content_occurrences()
            .flat_map(|field| field.content().inlines())
            .filter_map(|node| {
                if let InlineKind::CharacterReference(reference) = node.kind() {
                    Some(reference.kind().to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(references, ["&amp;", "&#169;"]);
    }
    Ok(())
}

#[test]
fn attribute_parts_keeps_caller_overrides_literal() -> Result<(), Error> {
    let options = Options::builder()
        .with_attribute("toc-title", "Caller &amp; <title>")
        .build()?;
    let parsed = parse(":toc-title: pass:c[Document]\n\nBody.", &options)?;
    let value = parsed
        .document()
        .attributes()
        .get("toc-title")
        .ok_or("missing title")?;
    assert_eq!(value.text(), Some("Caller &amp; <title>"));
    assert_eq!(spans(value), [("Caller &amp; <title>", 1)]);
    Ok(())
}

#[test]
fn attribute_parts_uses_neutral_text_for_conditions_and_paths() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    std::fs::write(directory.path().join("A & B.adoc"), "Included.\n")?;
    let options = Options::builder()
        .with_base_dir(directory.path())
        .with_safe_mode(acdc_parser::SafeMode::Unsafe)
        .build()?;
    let parsed = parse(
        ":label: pass:c[A & <B>]\n:chapter: pass:c[A & B]\n\nifeval::[\"{label}\" == \"A & <B>\"]\nMatched.\nendif::[]\n\ninclude::{chapter}.adoc[]\n",
        &options,
    )?;
    assert!(parsed.source().contains("Matched."));
    assert!(parsed.source().contains("Included."));
    assert!(!parsed.source().contains("include::"));
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
#[case("pass:[<é> &]", vec![])]
#[case("pass:c[<é> &]", vec![Substitution::SpecialChars])]
fn attributes_and_inline_passthroughs_share_a_text_piece_view(
    #[case] source: &str,
    #[case] substitutions: Vec<Substitution>,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        &format!(":value: {source}\n\n{source}"),
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let document = parsed.document();
    let value = document
        .attributes()
        .get("value")
        .ok_or("missing attribute")?;
    let parts = value.parts().map(read_part).collect::<Vec<_>>();
    assert_eq!(parts, [("<é> &", substitutions)]);
    let BlockKind::Paragraph(paragraph) =
        document.blocks().next().ok_or("missing paragraph")?.kind()
    else {
        return Err("expected paragraph".into());
    };
    let raw_parts = paragraph
        .content()
        .inlines()
        .map(|inline| {
            let InlineKind::Raw(raw) = inline.kind() else {
                return Err("expected lowered passthrough");
            };
            Ok(read_part(raw.text_part()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(raw_parts, parts);
    Ok(())
}

#[test]
fn attribute_parts_preserve_empty_values_and_integer_spelling() -> Result<(), Error> {
    let options = Options::builder()
        .with_attribute("max-include-depth", "0003")
        .build()?;
    let parsed = parse(":present:\n:empty: pass:[]\n\nBody.", &options)?;
    let attributes = parsed.document().attributes();
    let presence = attributes.get("present").ok_or("missing presence")?;
    assert!(presence.is_presence());
    assert_eq!(presence.text(), None);
    assert_eq!(presence.parts().count(), 0);
    let empty = attributes.get("empty").ok_or("missing empty value")?;
    assert_eq!(empty.text(), Some(""));
    assert_eq!(empty.parts().count(), 0);
    let integer = attributes
        .get("max-include-depth")
        .ok_or("missing integer")?;
    assert_eq!(integer.as_integer(), Some(3));
    assert_eq!(
        integer.parts().map(read_part).collect::<Vec<_>>(),
        [("0003", vec![Substitution::SpecialChars])]
    );
    Ok(())
}
