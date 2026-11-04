use crate::{Block, ContentAttribute, InlineNode, Options, Title, parse};

type Error = Box<dyn std::error::Error>;

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
