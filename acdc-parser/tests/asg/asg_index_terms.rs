#![cfg(feature = "asg")]

use acdc_parser::{
    CompatibilityMode, ContentAttribute, IndexTerm, IndexTermKind, IndexTermRelationship,
    InlineMacro, InlineNode, Location, Options, Plain, asg, parse, parse_file,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn content(term: IndexTerm<'_>) -> ContentAttribute<'_> {
    ContentAttribute::new(vec![InlineNode::Macro(InlineMacro::IndexTerm(Box::new(
        term,
    )))])
}

fn plain(text: &str) -> Vec<InlineNode<'_>> {
    vec![InlineNode::PlainText(Plain::new(text, Location::default()))]
}

#[test]
fn labels_hierarchy_relationships_and_controls_keep_their_own_fields() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/index_terms.adoc");
    let expected: Value =
        serde_json::from_str(&fs::read_to_string(source.with_extension("json"))?)?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse_file(
            &source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        assert_eq!(value, expected, "{mode:?}");
        assert_eq!(
            asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?,
            value
        );
        assert_eq!(serde_json::to_value(parsed.document())?, before);
        for (path, expected) in [
            ("/blocks/0/inlines/1/visibility", json!("visible")),
            ("/blocks/0/inlines/1/term/1/variant", json!("strong")),
            ("/blocks/1/inlines/1/visibility", json!("hidden")),
            ("/blocks/1/inlines/1/secondary/0/value", json!("Secondary")),
            ("/blocks/2/inlines/0/id", json!("entry")),
            ("/blocks/2/inlines/0/metadata/roles", json!(["one", "two"])),
            ("/blocks/2/inlines/0/metadata/options", json!(["a", "b"])),
            (
                "/blocks/2/inlines/0/metadata/attributes/custom",
                json!("kept"),
            ),
            ("/blocks/2/inlines/0/relationship/kind", json!("see")),
            ("/blocks/3/inlines/0/secondary", json!([])),
            ("/blocks/3/inlines/0/relationship/kind", json!("see-also")),
            ("/blocks/4/inlines/0/term", json!([])),
            ("/blocks/4/inlines/0/relationship/target", json!([])),
            (
                "/blocks/5/inlines/0/catalog/term/0/value",
                json!("{label} *label*"),
            ),
            (
                "/blocks/6/inlines/0/catalog/term/0/value",
                json!("Copyright (C)"),
            ),
            ("/blocks/6/inlines/0/term/1/value", json!("©")),
        ] {
            assert_eq!(value.pointer(path), Some(&expected), "{path}");
        }
        assert!(value.pointer("/blocks/0/inlines/1/catalog").is_none());
        assert!(matches!(
            asg::document(parsed.document()),
            Err(asg::Error::Unsupported {
                construct: Some("index term"),
                ..
            })
        ));
    }
    Ok(())
}

#[test]
fn prepared_display_edits_keep_catalog_labels_without_reparsing() -> Result<(), Error> {
    let mut term = IndexTerm::new(IndexTermKind::Flow(plain("*Old*")), Location::default());
    term.replace_kind(IndexTermKind::Flow(plain("*New*")));
    term.replace_relationship(Some(IndexTermRelationship::See { target: Vec::new() }));
    let content = content(term);
    let before = serde_json::to_value(&content)?;
    let value = asg::inlines_with_profile((&content).into(), asg::Profile::Acdc)?;
    assert_eq!(value.pointer("/0/term/0/value"), Some(&json!("*New*")));
    assert_eq!(
        value.pointer("/0/catalog/term/0/value"),
        Some(&json!("*Old*"))
    );
    assert_eq!(value.pointer("/0/relationship/target"), Some(&json!([])));
    assert!(value.pointer("/0/catalog/relationship").is_none());
    assert_eq!(serde_json::to_value(&content)?, before);
    Ok(())
}

#[test]
fn absent_and_empty_hierarchy_and_relationship_targets_remain_distinct() -> Result<(), Error> {
    let mut term = IndexTerm::new(
        IndexTermKind::Concealed {
            term: Vec::new(),
            secondary: None,
            tertiary: Some(Vec::new()),
        },
        Location::default(),
    );
    term.replace_relationship(Some(IndexTermRelationship::SeeAlso {
        targets: vec![Vec::new()],
    }));
    let content = content(term);
    let value = asg::inlines_with_profile((&content).into(), asg::Profile::Acdc)?;
    assert_eq!(value.pointer("/0/term"), Some(&json!([])));
    assert!(value.pointer("/0/secondary").is_none());
    assert_eq!(value.pointer("/0/tertiary"), Some(&json!([])));
    assert_eq!(value.pointer("/0/relationship/targets"), Some(&json!([[]])));
    Ok(())
}

#[test]
fn selected_titles_and_cells_keep_index_owners_and_footnote_registration() -> Result<(), Error> {
    let parsed = parse(
        "[title='((Title))']\nBody.\n\n[cols=a]\n|===\n|indexterm2:[Cell footnote:[Only note.]]\n|===",
        &Options::default(),
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert_eq!(
        value.pointer("/blocks/0/title/0/name"),
        Some(&json!("indexterm"))
    );
    assert_eq!(
        value.pointer("/blocks/1/rows/0/cells/0/blocks/0/inlines/0/name"),
        Some(&json!("indexterm"))
    );
    assert_eq!(
        value
            .pointer("/blocks/1/rows/0/cells/0/footnotes")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1)
    );
    assert!(value.get("footnotes").is_none());
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[test]
fn icon_label_children_keep_the_nested_owner_path() -> Result<(), Error> {
    let parsed = parse("indexterm2:[icon:heart[]]", &Options::default())?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert_eq!(
        value.pointer("/blocks/0/inlines/0/term/0/target"),
        Some(&json!("heart"))
    );
    Ok(())
}

#[test]
fn index_schema_closes_labels_snapshots_and_relationship_variants() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/index_terms.adoc");
    let parsed = parse_file(&source, &Options::default())?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let schema: Value = serde_json::from_str(include_str!("../../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&value));
    for (path, name, invalid) in [
        ("/blocks/0/inlines/1", "secondary", json!([])),
        ("/blocks/0/inlines/1", "visibility", json!("other")),
        ("/blocks/0/inlines/1", "term", json!("flattened")),
        ("/blocks/5/inlines/0/catalog", "unknown", json!(true)),
        ("/blocks/2/inlines/0/relationship", "targets", json!([])),
        ("/blocks/3/inlines/0/relationship", "target", json!([])),
        (
            "/blocks/3/inlines/0/relationship",
            "targets",
            json!(["flattened"]),
        ),
    ] {
        let mut changed = value.clone();
        changed
            .pointer_mut(path)
            .and_then(Value::as_object_mut)
            .ok_or("node")?
            .insert(name.into(), invalid);
        assert!(!validator.is_valid(&changed), "{path}/{name}");
    }
    Ok(())
}
