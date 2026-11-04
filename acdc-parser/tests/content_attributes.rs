use acdc_parser::{
    CompatibilityMode, Options, Title,
    ast::{BlockKind, InlineKind, InlineRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn content_attribute_preservation_registers_only_selected_footnotes() -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/content_attribute_preservation.adoc");
    let parsed = parse(source, &Options::default())?;
    let [selected, body] = &*parsed.document_ref().footnotes().collect::<Vec<_>>() else {
        return Err("expected only the selected caption and body footnotes".into());
    };
    assert_eq!((selected.number(), body.number()), (1, 2));
    for (note, expected) in [
        (selected, "footnote:[Selected caption.]"),
        (body, "footnote:[Body note.]"),
    ] {
        assert_eq!(
            source.get(note.location().absolute_start()..=note.location().absolute_end()),
            Some(expected)
        );
    }
    Ok(())
}

#[test]
fn title_presence_is_independent_of_inline_content() -> Result<(), Error> {
    let absent = Title::default();
    let empty = Title::new(Vec::new());
    assert!(absent.is_empty() && empty.is_empty());
    assert!(!absent.is_present());
    assert!(empty.is_present());
    assert_ne!(absent, empty);

    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = parse(
            "image::photo.png[title=\"\"]\n\nimage::photo.png[]",
            &options,
        )?;
        let json = serde_json::to_value(parsed.document_ref())?;
        assert_eq!(
            json.pointer("/blocks/0/title")
                .cloned()
                .ok_or("missing JSON value")?,
            serde_json::json!([])
        );
        assert!(
            json.pointer("/blocks/0/metadata/attributes/title")
                .is_none()
        );
        assert!(
            json.pointer("/blocks/1")
                .cloned()
                .ok_or("missing JSON value")?
                .get("title")
                .is_none()
        );
    }
    Ok(())
}

#[test]
fn named_titles_use_the_attributes_at_their_declaration() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for quote in ["'", "\""] {
            let source = format!(
                "= Title\n:word: Before\n\n[title={quote}{{word}} footnote:[Selected.]{quote}]\n:word: After\n{{word}}.\n"
            );
            let parsed = parse(&source, &options)?;
            let block = parsed
                .document_ref()
                .blocks()
                .find_map(|block| {
                    if let BlockKind::Paragraph(block) = block.kind() {
                        Some(block)
                    } else {
                        None
                    }
                })
                .ok_or("missing paragraph")?;

            if mode == CompatibilityMode::Draft && quote == "\"" {
                assert!(
                    matches!(&*block.title().ok_or("title")?.inlines().map(InlineRef::kind).collect::<Vec<_>>(), [InlineKind::Raw(text)]
                    if text.text() == "Before footnote:[Selected.]")
                );
            } else {
                assert!(
                    matches!(block.title().ok_or("title")?.inlines().next().map(InlineRef::kind), Some(InlineKind::Plain(text))
                    if text.text().starts_with("Before"))
                );
            }
            assert!(
                matches!(block.content().inlines().next().map(InlineRef::kind), Some(InlineKind::Plain(text)) if text.text() == "After.")
            );
            assert_eq!(
                parsed.document_ref().footnotes().count(),
                usize::from(quote == "'" || mode == CompatibilityMode::Asciidoctor)
            );
            for note in parsed.document_ref().footnotes() {
                assert_eq!(
                    source.get(note.location().absolute_start()..=note.location().absolute_end()),
                    Some("footnote:[Selected.]")
                );
            }
            let json = serde_json::to_value(parsed.document_ref())?;
            assert!(
                json.get("blocks")
                    .and_then(serde_json::Value::as_array)
                    .ok_or("blocks")?
                    .iter()
                    .all(|node| node.pointer("/metadata/attributes/title").is_none())
            );
        }
    }
    Ok(())
}
