#![cfg(feature = "asg")]

use acdc_parser::{
    Anchor, Block, CompatibilityMode, ContentAttribute, Document, Footnote, FootnoteDefinition,
    IndexTerm, IndexTermKind, InlineMacro, InlineNode, Location, Options, Paragraph, Plain,
    Reference, asg, parse, parse_file,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn text(value: &'static str) -> Vec<InlineNode<'static>> {
    vec![InlineNode::PlainText(Plain::new(
        value,
        Location::default(),
    ))]
}

fn note(number: u32, id: Option<&'static str>) -> InlineNode<'static> {
    InlineNode::Macro(InlineMacro::Footnote(Footnote::reference(
        id,
        number,
        Location::default(),
    )))
}

fn document(content: Vec<InlineNode<'static>>) -> Document<'static> {
    let mut document = Document::default();
    document.push_block(Block::Paragraph(Paragraph::new(
        content,
        Location::default(),
    )));
    document
}

fn captured(content: Vec<InlineNode<'static>>) -> InlineNode<'static> {
    let mut term = IndexTerm::new(IndexTermKind::Flow(content), Location::default());
    term.replace_kind(IndexTermKind::Flow(text("Current")));
    InlineNode::Macro(InlineMacro::IndexTerm(Box::new(term)))
}

fn export(document: &Document<'_>) -> Result<Value, asg::Error> {
    asg::document_with_profile(document.into(), asg::Profile::Acdc)
}

#[test]
fn combined_constructs_export_deterministically_without_mutation() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/integration.adoc");
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse_file(
            &source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        let expected: Value =
            serde_json::from_str(&fs::read_to_string(source.with_extension("json"))?)?;
        assert_eq!(value, expected, "{mode:?}");
        assert_eq!(
            asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?,
            value
        );
        assert_eq!(serde_json::to_value(parsed.document())?, before);
        assert_eq!(
            value.pointer("/blocks/0/inlines/0/id"),
            Some(&json!("real"))
        );
        assert_eq!(
            value.pointer("/footnotes/0/inlines/0/value"),
            Some(&json!("Canonical note."))
        );
        assert_eq!(
            value.pointer("/references/real/owner"),
            Some(&json!("/blocks/0/inlines/0"))
        );
    }
    Ok(())
}

#[test]
fn scalar_attributes_cannot_impersonate_identities_or_footnotes() -> Result<(), Error> {
    for source in [
        "= Guide\n:name: span\n:type: inline\n:id: real\n\n[#real]*Bold*",
        "= Guide\n:name: footnote\n:type: inline\n:number: 99\n:form: definition\n:id: real\n\n[#real]*Bold*",
    ] {
        let parsed = parse(source, &Options::default())?;
        let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        assert_eq!(value.pointer("/attributes/id"), Some(&json!("real")));
        assert_eq!(
            value.pointer("/blocks/0/inlines/0/id"),
            Some(&json!("real"))
        );
        assert_eq!(
            value.pointer("/references/real/owner"),
            Some(&json!("/blocks/0/inlines/0"))
        );
    }
    Ok(())
}

#[test]
fn captured_footnotes_require_matching_canonical_links() {
    for (number, id, definitions, suffix) in [
        (7, Some("note"), Vec::new(), "number"),
        (
            7,
            Some("other"),
            vec![FootnoteDefinition::new(Vec::new(), 7, Location::default())],
            "id",
        ),
    ] {
        let mut document = document(vec![captured(vec![note(number, id)])]);
        document.set_footnotes(definitions);
        assert!(
            matches!(export(&document), Err(asg::Error::Unsupported{path,construct:Some("footnote"),..}) if path == format!("/blocks/0/inlines/0/catalog/term/0/{suffix}"))
        );
    }
    let definition = InlineNode::Macro(InlineMacro::Footnote(Footnote::new(
        Vec::new(),
        0,
        Location::default(),
    )));
    assert!(
        matches!(export(&document(vec![captured(vec![definition])])), Err(asg::Error::Unsupported{path,..}) if path == "/blocks/0/inlines/0/catalog/term/0/number")
    );
}

#[test]
fn historical_bodies_do_not_replace_canonical_bodies_or_resolve_zero() -> Result<(), Error> {
    let mut old = Footnote::new(text("Old"), 7, Location::default());
    old.replace_id(Some("note"));
    let mut definition = FootnoteDefinition::new(text("Canonical"), 7, Location::default());
    definition.replace_id(Some("note"));
    let mut document = document(vec![captured(vec![
        InlineNode::Macro(InlineMacro::Footnote(old)),
        note(0, Some("unresolved")),
    ])]);
    document.set_footnotes([definition]);
    let before = serde_json::to_value(&document)?;
    let value = export(&document)?;
    assert_eq!(
        value.pointer("/footnotes/0/inlines/0/value"),
        Some(&json!("Canonical"))
    );
    assert_eq!(
        value.pointer("/blocks/0/inlines/0/catalog/term/0/number"),
        Some(&json!(7))
    );
    assert!(
        value
            .pointer("/blocks/0/inlines/0/catalog/term/0/inlines")
            .is_none()
    );
    assert_eq!(
        value.pointer("/blocks/0/inlines/0/catalog/term/1/number"),
        Some(&json!(0))
    );
    assert_eq!(serde_json::to_value(&document)?, before);
    Ok(())
}

#[test]
fn independent_labels_and_extra_anchors_cannot_hide_dangling_notes() -> Result<(), Error> {
    let parsed = parse("[#target]\nBody.", &Options::default())?;
    let primary = Reference::from(
        parsed
            .document()
            .references()
            .get("target")
            .ok_or("target")?,
    );
    let mut alias = primary.clone();
    alias.set_xreflabel(Some(ContentAttribute::new(vec![note(9, Some("missing"))])));
    let mut value = Document::from(parsed.document());
    value.set_references([("target", primary), ("alias/~", alias)]);
    assert!(
        matches!(export(&value), Err(asg::Error::Unsupported{path,..}) if path == "/references/alias~1~0/reftext/0/number")
    );
    let mut paragraph = Paragraph::new(Vec::new(), Location::default());
    let mut metadata = paragraph.take_metadata();
    metadata.id = Some(Anchor::new("primary", Location::default()));
    metadata.reftext = Some(ContentAttribute::new(text("Selected")));
    metadata.anchors = vec![
        Anchor::new("extra", Location::default())
            .with_xreflabel(Some(ContentAttribute::new(vec![note(9, Some("missing"))]))),
    ];
    paragraph.set_metadata(metadata)?;
    let mut value = Document::default();
    value.push_block(Block::Paragraph(paragraph));
    let result = export(&value);
    assert!(
        matches!(&result, Err(asg::Error::Unsupported{path,..}) if path == "/footnotes"),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn standalone_catalog_notes_still_require_document_context() {
    let content = ContentAttribute::new(vec![captured(vec![note(0, Some("unresolved"))])]);
    assert!(matches!(
        asg::inlines_with_profile((&content).into(), asg::Profile::Acdc),
        Err(asg::Error::Unsupported {
            construct: Some("footnote"),
            ..
        })
    ));
}

#[test]
fn strict_and_extension_controls_distinguish_shape_from_meaning() -> Result<(), Error> {
    let parsed = parse(
        "= Title\n\n// Source-only\n\n*Bold* and https://example.org[Link].",
        &Options::default(),
    )?;
    let strict = asg::document(parsed.document())?;
    let upstream: Value = serde_json::from_str(include_str!("../../schemas/asg/schema.json"))?;
    assert!(jsonschema::validator_for(&upstream)?.is_valid(&strict));
    let extended = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let schema: Value = serde_json::from_str(include_str!("../../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&extended));
    let mut wrong = extended.clone();
    *wrong.pointer_mut("/header/title/0/value").ok_or("title")? = json!("Wrong");
    assert!(validator.is_valid(&wrong));
    assert_ne!(wrong, extended);
    wrong
        .as_object_mut()
        .ok_or("document")?
        .insert("unknown".into(), json!(true));
    assert!(!validator.is_valid(&wrong));
    for source in [
        "----\nLiteral\n----",
        "See <<sample>>.\n\n[[sample]]\n.Title\n====\nBody.\n====",
    ] {
        let parsed = parse(source, &Options::default())?;
        assert!(asg::document(parsed.document()).is_err());
    }
    Ok(())
}
