use acdc_parser::{
    Admonition, AdmonitionForm, AdmonitionVariant, Block, CompatibilityMode, Location, Options,
    Paragraph, SafeMode,
    ast::{AdmonitionRef, BlockKind},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn native_admonitions_retain_real_source_forms(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/tests/admonition_source_forms.adoc");
    let parsed = parse_file(
        &source,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let value = serde_json::to_value(parsed.document())?;
    let blocks = value
        .get("blocks")
        .and_then(serde_json::Value::as_array)
        .ok_or("blocks")?;
    let expected = [
        ("paragraph", None),
        ("paragraph", None),
        ("indented", None),
        ("delimited", Some("======")),
        ("delimited", Some("--")),
        ("delimited", Some("========")),
    ];
    assert!(blocks.len() >= expected.len());
    for (block, (form, delimiter)) in blocks.iter().zip(expected) {
        assert_eq!(block.get("name"), Some(&serde_json::json!("admonition")));
        assert_eq!(block.get("form"), Some(&serde_json::json!(form)));
        assert_eq!(
            block.get("delimiter").and_then(serde_json::Value::as_str),
            delimiter
        );
    }
    Ok(())
}

#[rstest::rstest]
fn admonition_reads_and_prepared_copies_preserve_source_form(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    for (source, expected) in [
        ("NOTE: Text.", AdmonitionForm::Paragraph),
        ("[TIP]\nText.", AdmonitionForm::Paragraph),
        ("[IMPORTANT]\n    Text.", AdmonitionForm::Indented),
        (
            "[WARNING]\n======\nText.\n======",
            AdmonitionForm::Delimited("======"),
        ),
        ("[CAUTION]\n--\nText.\n--", AdmonitionForm::Delimited("--")),
    ] {
        let parsed = parse(
            source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let block = parsed.document().blocks().next().ok_or("admonition")?;
        let BlockKind::Admonition(admonition) = block.kind() else {
            return Err("admonition".into());
        };
        assert_eq!(admonition.form(), expected);
        assert_eq!(
            admonition.simple_paragraph().is_some(),
            matches!(
                expected,
                AdmonitionForm::Paragraph | AdmonitionForm::Indented
            )
        );
        let copied = Block::from(block);
        let Block::Admonition(mut copied) = copied else {
            return Err("prepared admonition".into());
        };
        assert_eq!(AdmonitionRef::from(&copied).form(), expected);
        assert_eq!(
            Some(&serde_json::to_value(&copied)?),
            serde_json::to_value(parsed.document())?.pointer("/blocks/0")
        );
        let children = copied.take_blocks().collect::<Vec<_>>();
        copied.set_blocks(children);
        assert_eq!(AdmonitionRef::from(&copied).form(), expected);
        assert_eq!(
            Some(&serde_json::to_value(&copied)?),
            serde_json::to_value(parsed.document())?.pointer("/blocks/0")
        );
    }
    Ok(())
}

#[test]
fn constructed_admonitions_do_not_invent_source_delimiters() -> Result<(), Error> {
    let mut owner = Admonition::new(AdmonitionVariant::Note, Vec::new(), Location::default());
    assert_eq!(
        AdmonitionRef::from(&owner).form(),
        AdmonitionForm::Unspecified
    );
    assert!(serde_json::to_value(&owner)?.get("form").is_none());
    owner.set_form(AdmonitionForm::Delimited("--"));
    assert_eq!(
        serde_json::to_value(&owner)?.get("delimiter"),
        Some(&serde_json::json!("--"))
    );
    let paragraph = Admonition::from_paragraph(
        AdmonitionVariant::Tip,
        Paragraph::new(Vec::new(), Location::default()),
        Location::default(),
    );
    assert_eq!(
        AdmonitionRef::from(&paragraph).form(),
        AdmonitionForm::Paragraph
    );
    let owner = owner.with_form(AdmonitionForm::Paragraph);
    assert!(serde_json::to_value(owner)?.get("delimiter").is_none());
    Ok(())
}
