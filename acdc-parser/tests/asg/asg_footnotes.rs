#![cfg(feature = "asg")]

use acdc_parser::{
    Block, CompatibilityMode, Document, Footnote, FootnoteDefinition, InlineMacro, InlineNode,
    Location, Options, Paragraph, Plain, asg, parse, parse_file, parse_inline,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn document(
    notes: Vec<FootnoteDefinition<'static>>,
    occurrences: Vec<Footnote<'static>>,
) -> Document<'static> {
    let mut document = Document::default();
    document.set_footnotes(notes);
    document.push_block(Block::Paragraph(Paragraph::new(
        occurrences
            .into_iter()
            .map(|note| InlineNode::Macro(InlineMacro::Footnote(note)))
            .collect(),
        Location::default(),
    )));
    document
}

#[test]
fn registered_bodies_have_one_owner_and_occurrences_keep_identity() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/footnotes.adoc");
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
        assert_eq!(
            value
                .get("footnotes")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(3)
        );
        assert_eq!(value.pointer("/footnotes/1/id"), Some(&json!("n")));
        assert_eq!(
            value.pointer("/footnotes/0/inlines/1/variant"),
            Some(&json!("strong"))
        );
        assert_eq!(value.pointer("/footnotes/2/id"), Some(&json!("title")));
        let body = value
            .pointer("/blocks/0/inlines")
            .and_then(Value::as_array)
            .ok_or("body")?;
        let notes = body
            .iter()
            .filter(|node| node.get("name") == Some(&json!("footnote")))
            .collect::<Vec<_>>();
        assert_eq!(
            notes
                .iter()
                .map(|note| note.get("number").cloned())
                .collect::<Vec<_>>(),
            [Some(json!(1)), Some(json!(2)), Some(json!(2))]
        );
        assert!(notes.iter().all(|note| note.get("inlines").is_none()));
        assert!(
            body.iter()
                .filter_map(|node| node.get("value").and_then(Value::as_str))
                .any(|text| text.contains("[missing]"))
        );
        assert!(
            matches!(asg::document(parsed.document()),Err(asg::Error::Unsupported{path,..}) if path=="/footnotes/0")
        );
    }
    Ok(())
}

#[test]
fn empty_and_catalog_only_definitions_keep_number_and_presence() -> Result<(), Error> {
    let mut definition = FootnoteDefinition::new(Vec::new(), 7, Location::default());
    definition.replace_id(Some("empty"));
    let mut note = Footnote::new(Vec::new(), 7, Location::default());
    note.replace_id(Some("empty"));
    let document = document(
        vec![definition.clone()],
        vec![
            note,
            Footnote::reference(Some("empty"), 7, Location::default()),
            Footnote::reference(Some("empty"), 0, Location::default()),
        ],
    );
    let value = asg::document_with_profile((&document).into(), asg::Profile::Acdc)?;
    assert_eq!(
        value.get("footnotes"),
        Some(&json!([{"number":7,"id":"empty","inlines":[]}]))
    );
    assert_eq!(
        value.pointer("/blocks/0/inlines/0/form"),
        Some(&json!("definition"))
    );
    assert_eq!(
        value.pointer("/blocks/0/inlines/1/form"),
        Some(&json!("reference"))
    );
    assert_eq!(value.pointer("/blocks/0/inlines/2/number"), Some(&json!(0)));
    let mut only = Document::default();
    only.set_footnotes([definition]);
    let only = asg::document_with_profile((&only).into(), asg::Profile::Acdc)?;
    assert_eq!(only.get("footnotes"), value.get("footnotes"));
    Ok(())
}

#[test]
fn stale_and_duplicate_registrations_reject_without_repair() -> Result<(), Error> {
    let definition = FootnoteDefinition::new(Vec::new(), 1, Location::default());
    let text = vec![InlineNode::PlainText(Plain::new(
        "Different",
        Location::default(),
    ))];
    for document in [
        document(
            Vec::new(),
            vec![Footnote::new(Vec::new(), 1, Location::default())],
        ),
        document(
            vec![definition.clone()],
            vec![Footnote::new(text, 1, Location::default())],
        ),
        document(vec![definition.clone(), definition], Vec::new()),
        document(
            vec![FootnoteDefinition::new(Vec::new(), 0, Location::default())],
            Vec::new(),
        ),
        document(
            vec![
                FootnoteDefinition::new(
                    vec![InlineNode::Macro(InlineMacro::Footnote(
                        Footnote::reference(None, 2, Location::default()),
                    ))],
                    1,
                    Location::default(),
                ),
                FootnoteDefinition::new(
                    vec![InlineNode::Macro(InlineMacro::Footnote(
                        Footnote::reference(None, 1, Location::default()),
                    ))],
                    2,
                    Location::default(),
                ),
            ],
            Vec::new(),
        ),
    ] {
        let before = serde_json::to_value(&document)?;
        assert!(
            matches!(asg::document_with_profile((&document).into(),asg::Profile::Acdc),Err(asg::Error::Unsupported{path,..}) if path.starts_with("/footnotes"))
        );
        assert_eq!(serde_json::to_value(&document)?, before);
    }
    Ok(())
}

#[test]
fn selected_titles_and_nested_cells_preserve_existing_registration_order() -> Result<(), Error> {
    let parsed = parse(
        "[title='Old footnote:discard[Discarded]']\n.Title footnote:keep[Kept]\nText.\n\n[cols=a]\n|===\n|Cell footnote:cell[Cell note.] and footnote:keep[].\n|===",
        &Options::default(),
    )?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let definitions = value
        .get("footnotes")
        .and_then(Value::as_array)
        .ok_or("notes")?;
    assert_eq!(
        definitions
            .iter()
            .map(|value| value.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>(),
        [Some("keep")]
    );
    assert_eq!(
        value.pointer("/blocks/1/rows/0/cells/0/blocks/0/inlines/1/number"),
        Some(&json!(2))
    );
    Ok(())
}

#[test]
fn canonical_body_icons_and_standalone_context_are_explicit() -> Result<(), Error> {
    let parsed = parse("Text footnote:[icon:heart[]].", &Options::default())?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert_eq!(
        value.pointer("/footnotes/0/inlines/0/target"),
        Some(&json!("heart"))
    );
    let parsed = parse_inline("*Text footnote:[Note]*", &Options::default())?;
    assert!(matches!(
        asg::inlines_with_profile(parsed.content(), asg::Profile::Acdc),
        Err(asg::Error::Unsupported {
            construct: Some("footnote"),
            ..
        })
    ));
    Ok(())
}

#[test]
fn footnote_schema_keeps_closed_bodies_and_linked_occurrences() -> Result<(), Error> {
    let parsed = parse("Text footnote:[Note].", &Options::default())?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let schema: Value = serde_json::from_str(include_str!("../../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&value));
    for (path, name, invalid) in [
        ("/footnotes/0", "number", json!(0)),
        ("/footnotes/0", "inlines", json!("flattened")),
        ("/footnotes/0", "unknown", json!(true)),
        ("/blocks/0/inlines/1", "number", json!(-1)),
        ("/blocks/0/inlines/1", "form", json!("other")),
        ("/blocks/0/inlines/1", "inlines", json!([])),
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
