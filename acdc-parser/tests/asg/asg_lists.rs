//! List export preserves semantic items and rejects properties without a mapping.
#![cfg(feature = "asg")]

use acdc_parser::{CompatibilityMode, Options, asg, parse};
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
            let value = asg::export_document(parsed.document())?;
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
        let value = asg::export_document(parsed.document())?;
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
    let value = asg::export_document(parsed.document())?;
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
    let value = asg::export_document(parsed.document())?;
    assert_eq!(
        value.pointer("/blocks/0/items/0/terms/0/0/variant"),
        Some(&json!("strong"))
    );
    Ok(())
}
