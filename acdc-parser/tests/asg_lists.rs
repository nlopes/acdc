//! List export preserves semantic items and rejects properties without a mapping.
#![cfg(feature = "asg")]

use acdc_parser::{
    Anchor, Block, CalloutRef, CompatibilityMode, Document, InlineNode, Location, Options, asg,
    parse,
};
use serde_json::json;

type Error = Box<dyn std::error::Error + Send + Sync>;

#[test]
fn each_list_family_preserves_its_items_and_markers() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (source, name, variant, marker, second_marker) in [
            (
                "* First *bold*\n* Second",
                "list",
                Some("unordered"),
                "*",
                "*",
            ),
            (
                "1. First *bold*\n2. Second",
                "list",
                Some("ordered"),
                "1.",
                "2.",
            ),
            (
                "<1> First *bold*\n<.> Second",
                "list",
                Some("callout"),
                "<1>",
                "<.>",
            ),
            (
                "First:: First *bold*\nSecond:: Second",
                "dlist",
                None,
                "::",
                "::",
            ),
        ] {
            let parsed = parse(source, &options)?;
            let before = serde_json::to_value(parsed.document())?;
            let value = asg::document(parsed.document())?;
            assert_eq!(
                value.pointer("/blocks/0/name"),
                Some(&json!(name)),
                "{source}"
            );
            assert_eq!(
                value.pointer("/blocks/0/variant"),
                variant.map(|v| json!(v)).as_ref()
            );
            assert_eq!(value.pointer("/blocks/0/marker"), Some(&json!(marker)));
            assert_eq!(
                value.pointer("/blocks/0/items/1/marker"),
                Some(&json!(second_marker))
            );
            assert_eq!(
                value.pointer("/blocks/0/items/0/principal/1/variant"),
                Some(&json!("strong"))
            );
            assert_eq!(
                value.pointer("/blocks/0/items/1/principal/0/value"),
                Some(&json!("Second"))
            );
            if name == "dlist" {
                assert_eq!(
                    value.pointer("/blocks/0/items/0/name"),
                    Some(&json!("dlistItem"))
                );
                assert_eq!(
                    value.pointer("/blocks/0/items/0/terms/0/0/value"),
                    Some(&json!("First"))
                );
            } else {
                assert_eq!(
                    value.pointer("/blocks/0/items/0/name"),
                    Some(&json!("listItem"))
                );
            }
            assert_eq!(
                value.pointer("/blocks/0/items/0/location"),
                before.pointer("/blocks/0/items/0/location")
            );
            assert_eq!(serde_json::to_value(parsed.document())?, before);
        }
    }
    Ok(())
}

#[test]
fn list_topology_and_attached_blocks_are_preserved() -> Result<(), Error> {
    for (source, pointer, expected) in [
        (
            "* Outer\n** Child\n* Last",
            "/blocks/0/items/0/blocks/0/items/0/principal/0/value",
            "Child",
        ),
        (
            "* First\n+\nAttached paragraph.\n\n* Second",
            "/blocks/0/items/0/blocks/0/inlines/0/value",
            "Attached paragraph.",
        ),
        (
            "Term:: Definition\n+\nMore detail.",
            "/blocks/0/items/0/blocks/0/inlines/0/value",
            "More detail.",
        ),
        (
            "Parent:: Definition\nChild::: Nested",
            "/blocks/0/items/0/blocks/0/items/0/terms/0/0/value",
            "Child",
        ),
    ] {
        let parsed = parse(source, &Options::default())?;
        let value = asg::document(parsed.document())?;
        assert_eq!(value.pointer(pointer), Some(&json!(expected)), "{source}");
    }
    Ok(())
}

#[test]
fn list_metadata_and_formatted_description_terms_keep_their_meaning() -> Result<(), Error> {
    let parsed = parse(
        "[#tasks,reftext='Task *list*']\n.Tasks\n[.compact]\n* First",
        &Options::default(),
    )?;
    let value = asg::document(parsed.document())?;
    assert_eq!(value.pointer("/blocks/0/id"), Some(&json!("tasks")));
    assert_eq!(
        value.pointer("/blocks/0/title/0/value"),
        Some(&json!("Tasks"))
    );
    assert_eq!(
        value.pointer("/blocks/0/reftext/1/variant"),
        Some(&json!("strong"))
    );
    assert_eq!(
        value.pointer("/blocks/0/metadata/roles"),
        Some(&json!(["compact"]))
    );
    let parsed = parse("*Term*:: Definition", &Options::default())?;
    let value = asg::document(parsed.document())?;
    assert_eq!(
        value.pointer("/blocks/0/items/0/terms/0/0/variant"),
        Some(&json!("strong"))
    );
    Ok(())
}

#[test]
fn invalid_list_state_fails_before_schema_projection() -> Result<(), Error> {
    for source in ["* Item", ". Item", "<1> Item", "Term:: Definition"] {
        let parsed = parse(source, &Options::default())?;
        let mut document = Document::default();
        document.blocks.clone_from(&parsed.document().blocks);
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
            matches!(asg::document(&document), Err(asg::Error::Unsupported {path, ..}) if path == "/blocks/0/items")
        );
    }
    let parsed = parse("* Item", &Options::default())?;
    let mut document = Document::default();
    document.blocks.clone_from(&parsed.document().blocks);
    let Some(Block::UnorderedList(list)) = document.blocks.first_mut() else {
        return Err("list".into());
    };
    list.items.first_mut().ok_or("item")?.level = 4;
    assert!(
        matches!(asg::document(&document), Err(asg::Error::Unsupported {path, construct: Some("list item"), ..})
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
    document.blocks.clone_from(&parsed.document().blocks);
    let Some(Block::UnorderedList(list)) = document.blocks.first_mut() else {
        return Err("list".into());
    };
    list.items.first_mut().ok_or("item")?.principal = vec![InlineNode::CalloutRef(
        CalloutRef::explicit(1, Location::default()),
    )];
    assert!(
        matches!(asg::document(&document), Err(asg::Error::Unsupported {path, construct: Some("callout"), ..})
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
    document.blocks.clone_from(&parsed.document().blocks);
    let Some(Block::CalloutList(list)) = document.blocks.first_mut() else {
        return Err("list".into());
    };
    list.items.first_mut().ok_or("item")?.callout.number = 42;
    assert!(
        matches!(asg::document(&document), Err(asg::Error::Unsupported {path, ..}) if path == "/blocks/0/items/0/callout/number")
    );
    Ok(())
}

#[test]
fn description_item_identities_are_preserved_or_explicitly_rejected() -> Result<(), Error> {
    let parsed = parse("Term:: Definition", &Options::default())?;
    let mut document = Document::default();
    document.blocks.clone_from(&parsed.document().blocks);
    let Some(Block::DescriptionList(list)) = document.blocks.first_mut() else {
        return Err("list".into());
    };
    list.items
        .first_mut()
        .ok_or("item")?
        .anchors
        .push(Anchor::new("term-id", Location::default()));
    let value = asg::document(&document)?;
    assert_eq!(
        value.pointer("/blocks/0/items/0/id"),
        Some(&json!("term-id"))
    );
    let Some(Block::DescriptionList(list)) = document.blocks.first_mut() else {
        return Err("list".into());
    };
    list.items
        .first_mut()
        .ok_or("item")?
        .anchors
        .push(Anchor::new("alias", Location::default()));
    assert!(
        matches!(asg::document(&document), Err(asg::Error::Unsupported {path, ..}) if path == "/blocks/0/items/0/anchors")
    );
    Ok(())
}

#[test]
fn list_attachments_cannot_gain_sections() -> Result<(), Error> {
    let parsed = parse("* Item", &Options::default())?;
    let mut document = Document::default();
    document.blocks.clone_from(&parsed.document().blocks);
    let Some(Block::UnorderedList(list)) = document.blocks.first_mut() else {
        return Err("list".into());
    };
    list.items
        .first_mut()
        .ok_or("item")?
        .blocks
        .push(Block::Section(acdc_parser::Section::new(
            acdc_parser::Title::new(Vec::new()),
            1,
            Vec::new(),
            Location::default(),
        )));
    assert!(
        matches!(asg::document(&document), Err(asg::Error::Unsupported {path, construct: Some("section"), ..})
        if path == "/blocks/0/items/0/blocks/0")
    );
    Ok(())
}
