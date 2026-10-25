use acdc_parser::{Block, CompatibilityMode, ContentAttribute, InlineNode, Options, Title, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn content_attribute_preservation_registers_only_selected_footnotes() -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/content_attribute_preservation.adoc");
    let parsed = parse(source, &Options::default())?;
    let [selected, body] = parsed.document().footnotes.as_slice() else {
        return Err("expected only the selected caption and body footnotes".into());
    };
    assert_eq!((selected.number, body.number), (1, 2));
    for (note, expected) in [
        (selected, "footnote:[Selected caption.]"),
        (body, "footnote:[Body note.]"),
    ] {
        assert_eq!(
            source.get(note.location.absolute_start..=note.location.absolute_end),
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
        let json = serde_json::to_value(parsed.document())?;
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
fn replacing_a_title_cannot_leave_its_old_named_value() -> Result<(), Error> {
    let parsed = parse("[title='*Old*',custom=kept]\nText.", &Options::default())?;
    let [Block::Paragraph(original)] = parsed.document().blocks.as_slice() else {
        return Err("missing paragraph".into());
    };
    assert!(matches!(
        original.title.first(),
        Some(InlineNode::BoldText(_))
    ));
    assert!(!original.metadata.attributes.contains_key("title"));

    let mut changed = original.clone();
    changed.title = Title::new(Vec::new());
    let json = serde_json::to_value(&changed)?;
    assert!(
        json.pointer("/metadata/attributes")
            .cloned()
            .ok_or("missing JSON value")?
            .get("title")
            .is_none()
    );
    assert_eq!(
        json.pointer("/metadata/attributes/custom")
            .cloned()
            .ok_or("missing JSON value")?,
        "kept"
    );
    assert_eq!(
        json.pointer("/title")
            .cloned()
            .ok_or("missing JSON value")?,
        serde_json::json!([])
    );
    changed.title = Title::default();
    assert!(serde_json::to_value(&changed)?.get("title").is_none());
    Ok(())
}

#[test]
fn content_attributes_serialize_only_their_selected_inline_content() -> Result<(), Error> {
    let parsed = parse(
        "[title=Discarded]\n.Selected\n[quote,attribution='*Ada*',citetitle='A _book_']\n____\nWords.\n____",
        &Options::default(),
    )?;
    let [Block::DelimitedBlock(block)] = parsed.document().blocks.as_slice() else {
        return Err("missing quote".into());
    };
    let attribution = block
        .metadata
        .attribution
        .as_ref()
        .ok_or("missing attribution")?;
    let citation = block
        .metadata
        .citetitle
        .as_ref()
        .ok_or("missing citation")?;
    assert!(matches!(attribution.first(), Some(InlineNode::BoldText(_))));
    assert!(
        citation
            .iter()
            .any(|node| matches!(node, InlineNode::ItalicText(_)))
    );
    assert_eq!(attribution, &ContentAttribute::new(attribution.to_vec()));
    assert_eq!(citation, &ContentAttribute::new(citation.to_vec()));

    let json = serde_json::to_value(block)?;
    assert!(json.pointer("/metadata/attributes").is_none());
    assert_eq!(
        json.pointer("/title/0/value"),
        Some(&serde_json::json!("Selected"))
    );
    assert_eq!(
        json.pointer("/metadata/attribution"),
        Some(&serde_json::to_value(attribution)?)
    );
    assert_eq!(
        json.pointer("/metadata/citetitle"),
        Some(&serde_json::to_value(citation)?)
    );
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
                .document()
                .blocks
                .iter()
                .find_map(|block| {
                    if let Block::Paragraph(block) = block {
                        Some(block)
                    } else {
                        None
                    }
                })
                .ok_or("missing paragraph")?;

            if mode == CompatibilityMode::Draft && quote == "\"" {
                assert!(matches!(block.title.as_ref(), [InlineNode::RawText(text)]
                    if text.content == "Before footnote:[Selected.]"));
            } else {
                assert!(
                    matches!(block.title.first(), Some(InlineNode::PlainText(text))
                    if text.content.starts_with("Before"))
                );
            }
            assert!(
                matches!(block.content.first(), Some(InlineNode::PlainText(text)) if text.content == "After.")
            );
            assert_eq!(
                parsed.document().footnotes.len(),
                usize::from(quote == "'" || mode == CompatibilityMode::Asciidoctor)
            );
            for note in &parsed.document().footnotes {
                assert_eq!(
                    source.get(note.location.absolute_start..=note.location.absolute_end),
                    Some("footnote:[Selected.]")
                );
            }
            let json = serde_json::to_value(block)?;
            assert!(json.pointer("/metadata/attributes/title").is_none());
        }
    }
    Ok(())
}
