use crate::{Anchor, Block, CalloutRef, Document, InlineNode, Location, Options, asg, parse};
use serde_json::json;

type Error = Box<dyn std::error::Error + Send + Sync>;

#[test]
fn invalid_list_state_fails_before_schema_projection() -> Result<(), Error> {
    for source in ["* Item", ". Item", "<1> Item", "Term:: Definition"] {
        let parsed = parse(source, &Options::default())?;
        let mut document = Document::default();
        document
            .blocks
            .clone_from(&parsed.document_storage().blocks);
        let block = document.blocks.first_mut().ok_or("list")?;
        if let Block::UnorderedList(list) = block {
            list.items.clear();
        } else if let Block::OrderedList(list) = block {
            list.items.clear();
        } else if let Block::CalloutList(list) = block {
            list.items.clear();
        } else if let Block::DescriptionList(list) = block {
            list.items.clear();
        } else {
            return Err("unexpected list type".into());
        }
        assert!(
            matches!(asg::document((&document).into()), Err(asg::Error::Unsupported {path, ..}) if path == "/blocks/0/items")
        );
    }
    let parsed = parse("* Item", &Options::default())?;
    let mut document = Document::default();
    document
        .blocks
        .clone_from(&parsed.document_storage().blocks);
    let Some(Block::UnorderedList(list)) = document.blocks.first_mut() else {
        return Err("list".into());
    };
    list.items.first_mut().ok_or("item")?.level = 4;
    assert!(
        matches!(asg::document((&document).into()), Err(asg::Error::Unsupported {path, construct: Some("list item"), ..})
        if path == "/blocks/0/items/0/level")
    );
    Ok(())
}

#[test]
fn checklist_and_nested_unsupported_content_report_their_own_paths() -> Result<(), Error> {
    for (source, expected, owner) in [
        ("* [x] Done", "/blocks/0/items/0/checked", "list item"),
        (
            "* First\n** image:photo.png[Alt]",
            "/blocks/0/items/0/blocks/0/items/0/principal/0",
            "inline image",
        ),
        (
            "image:term.png[Alt]:: Definition",
            "/blocks/0/items/0/terms/0/0",
            "inline image",
        ),
        (
            "Term:: image:photo.png[Alt]",
            "/blocks/0/items/0/principal/0",
            "inline image",
        ),
    ] {
        let parsed = parse(source, &Options::default())?;
        assert!(
            matches!(asg::document(parsed.document()), Err(asg::Error::Unsupported {path, construct: Some(actual), ..})
            if path == expected && actual == owner),
            "{source}"
        );
    }
    let parsed = parse("* Item", &Options::default())?;
    let mut document = Document::default();
    document
        .blocks
        .clone_from(&parsed.document_storage().blocks);
    let Some(Block::UnorderedList(list)) = document.blocks.first_mut() else {
        return Err("list".into());
    };
    list.items.first_mut().ok_or("item")?.principal = vec![InlineNode::CalloutRef(
        CalloutRef::explicit(1, Location::default()),
    )];
    assert!(
        matches!(asg::document((&document).into()), Err(asg::Error::Unsupported {path, construct: Some("callout"), ..})
        if path == "/blocks/0/items/0/principal/0")
    );
    Ok(())
}

#[test]
fn callout_markers_preserve_kind_and_resolved_number() -> Result<(), Error> {
    let parsed = parse("<.> First\n<5> Second\n<.> Third", &Options::default())?;
    let value = asg::document(parsed.document())?;
    assert_eq!(value.pointer("/blocks/0/marker"), Some(&json!("<.>")));
    assert_eq!(
        value.pointer("/blocks/0/items/0/marker"),
        Some(&json!("<.>"))
    );
    assert_eq!(
        value.pointer("/blocks/0/items/1/marker"),
        Some(&json!("<2>"))
    );
    assert_eq!(
        value.pointer("/blocks/0/items/2/marker"),
        Some(&json!("<.>"))
    );
    let mut document = Document::default();
    document
        .blocks
        .clone_from(&parsed.document_storage().blocks);
    let Some(Block::CalloutList(list)) = document.blocks.first_mut() else {
        return Err("list".into());
    };
    list.items.first_mut().ok_or("item")?.callout.number = 42;
    assert!(
        matches!(asg::document((&document).into()), Err(asg::Error::Unsupported {path, ..}) if path == "/blocks/0/items/0/callout/number")
    );
    Ok(())
}

#[test]
fn description_item_identities_are_preserved_or_explicitly_rejected() -> Result<(), Error> {
    let parsed = parse("Term:: Definition", &Options::default())?;
    let mut document = Document::default();
    document
        .blocks
        .clone_from(&parsed.document_storage().blocks);
    let Some(Block::DescriptionList(list)) = document.blocks.first_mut() else {
        return Err("list".into());
    };
    let term = list
        .items
        .first_mut()
        .ok_or("item")?
        .terms
        .first_mut()
        .ok_or("term")?;
    term.anchors
        .push(Anchor::new("term-id", Location::default()));
    term.anchors.push(Anchor::new("alias", Location::default()));
    assert!(
        matches!(asg::document((&document).into()), Err(asg::Error::Unsupported {path, ..}) if path == "/blocks/0/items/0/terms/0")
    );
    let value = asg::document_with_profile((&document).into(), asg::Profile::Acdc)?;
    assert_eq!(
        value.pointer("/blocks/0/items/0/term_metadata/0/id"),
        Some(&json!("term-id"))
    );
    assert_eq!(
        value.pointer("/blocks/0/items/0/term_metadata/0/anchors/0/id"),
        Some(&json!("alias"))
    );
    Ok(())
}

#[test]
fn list_attachments_cannot_gain_sections() -> Result<(), Error> {
    let parsed = parse("* Item", &Options::default())?;
    let mut document = Document::default();
    document
        .blocks
        .clone_from(&parsed.document_storage().blocks);
    let Some(Block::UnorderedList(list)) = document.blocks.first_mut() else {
        return Err("list".into());
    };
    list.items
        .first_mut()
        .ok_or("item")?
        .blocks
        .push(Block::Section(crate::Section::new(
            crate::Title::new(Vec::new()),
            1,
            Vec::new(),
            Location::default(),
        )));
    assert!(
        matches!(asg::document((&document).into()), Err(asg::Error::Unsupported {path, construct: Some("section"), ..})
        if path == "/blocks/0/items/0/blocks/0")
    );
    Ok(())
}
