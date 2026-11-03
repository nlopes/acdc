use acdc_parser::{
    AttributeValue, Block, BlockMetadata, CompatibilityMode, ContentAttribute, InlineNode, Options,
    Raw, Substitution, Title, parse,
};
use serde_json::{Value, json};

type TestError = Box<dyn std::error::Error>;

#[rstest::rstest]
fn document_json_keeps_semantic_types_and_body_order(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), TestError> {
    let source = "= Values\n:toc:\n:toclevels: 03\n:max-include-depth: 03\n:custom-presence:\n:custom-number-text: 007\n\nBody.\n\n:toc!:\n:toc:\n:toclevels: 02\n:max-include-depth: 02\n:custom-presence!:\n:custom-presence:\n\nAfter.\n";
    let parsed = parse(
        source,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_attribute("max-include-depth", "03")
            .build()?,
    )?;
    let doc = parsed.document();
    assert_eq!(
        doc.attributes
            .get("max-include-depth")
            .and_then(acdc_parser::DocumentAttributeValue::as_integer),
        Some(3)
    );
    assert_eq!(
        doc.attributes
            .get("max-include-depth")
            .and_then(acdc_parser::DocumentAttributeValue::text),
        Some("03")
    );
    let value = serde_json::to_value(doc)?;
    assert_eq!(value.pointer("/attributes/toc"), Some(&json!(true)));
    assert_eq!(
        value.pointer("/attributes/max-include-depth"),
        Some(&json!(3))
    );
    assert_eq!(value.pointer("/attributes/toclevels"), Some(&json!("03")));
    assert_eq!(
        value.pointer("/attributes/custom-presence"),
        Some(&json!(true))
    );
    assert_eq!(
        value.pointer("/attributes/custom-number-text"),
        Some(&json!("007"))
    );
    let events = value
        .get("blocks")
        .and_then(Value::as_array)
        .ok_or("blocks")?
        .iter()
        .filter(|node| node.get("type").and_then(Value::as_str) == Some("attribute"))
        .map(|node| (node.get("name").cloned(), node.get("value").cloned()))
        .collect::<Vec<_>>();
    assert_eq!(
        events,
        [
            (Some(json!("toc")), Some(json!(false))),
            (Some(json!("toc")), Some(json!(true))),
            (Some(json!("toclevels")), Some(json!("02"))),
            (Some(json!("custom-presence")), Some(json!(false))),
            (Some(json!("custom-presence")), Some(json!(true))),
        ]
    );
    Ok(())
}

#[test]
fn scalar_metadata_does_not_apply_document_toc_encoding() -> Result<(), TestError> {
    let mut metadata = BlockMetadata::default();
    metadata
        .attributes
        .set("toc".into(), AttributeValue::Bool(true));
    let attributes = serde_json::to_value(&metadata.attributes)?;
    assert_eq!(attributes.get("toc"), Some(&json!(true)));
    assert_eq!(
        serde_json::to_value(&metadata)?.get("attributes"),
        Some(&attributes)
    );
    Ok(())
}

#[rstest::rstest]
#[case("id")]
#[case("role")]
#[case("roles")]
#[case("opts")]
#[case("options")]
#[case("style")]
#[case("title")]
#[case("caption")]
#[case("reftext")]
#[case("subs")]
#[case("attribution")]
#[case("citetitle")]
fn stale_reserved_block_attributes_cannot_be_serialized(
    #[case] name: &str,
) -> Result<(), TestError> {
    let mut metadata = BlockMetadata::default();
    metadata.attributes.set(name.into(), "stale".into());
    let error = serde_json::to_value(metadata)
        .err()
        .ok_or("reserved scalar must not shadow its owner")?;
    assert!(error.to_string().contains(name));
    Ok(())
}

#[test]
fn block_image_alt_cannot_reappear_from_the_scalar_map() -> Result<(), TestError> {
    let parsed = parse("image::photo.png[Alt]", &Options::default())?;
    let [Block::Image(owner)] = parsed.document().blocks.as_slice() else {
        return Err("image".into());
    };
    let mut owner = owner.clone();
    owner.alt = None;
    owner.metadata.attributes.set("alt".into(), "stale".into());
    assert!(serde_json::to_value(owner).is_err());
    Ok(())
}

#[test]
fn content_replacement_and_removal_do_not_resurrect_old_values() -> Result<(), TestError> {
    let parsed = parse("image::photo.png[Alt,title=Title]", &Options::default())?;
    let [Block::Image(owner)] = parsed.document().blocks.as_slice() else {
        return Err("image".into());
    };
    let mut owner = owner.clone();
    let original = serde_json::to_value(&owner)?;
    assert!(original.get("alt").is_some());
    owner.alt = Some(ContentAttribute::default());
    owner.title = Title::new(Vec::new());
    let empty = serde_json::to_value(&owner)?;
    assert_eq!(empty.get("alt"), Some(&json!([])));
    assert_eq!(empty.get("title"), Some(&json!([])));
    owner.alt = None;
    owner.title = Title::default();
    let absent = serde_json::to_value(&owner)?;
    assert!(absent.get("alt").is_none());
    assert!(absent.get("title").is_none());
    assert!(absent.pointer("/metadata/attributes/alt").is_none());
    assert!(absent.pointer("/metadata/attributes/title").is_none());
    assert_eq!(
        original,
        serde_json::to_value(parsed.document().blocks.first().ok_or("original image")?)?
    );
    Ok(())
}

#[rstest::rstest]
#[case(vec![])]
#[case(vec![Substitution::SpecialChars])]
#[case(vec![Substitution::Replacements, Substitution::SpecialChars, Substitution::Replacements])]
fn raw_json_retains_its_independent_substitution_policy(
    #[case] substitutions: Vec<Substitution>,
) -> Result<(), TestError> {
    let raw = Raw {
        content: "<literal> &copy;",
        location: acdc_parser::Location::default(),
        subs: substitutions.clone(),
    };
    let direct = serde_json::to_value(&raw)?;
    let value = serde_json::to_value(InlineNode::RawText(raw))?;
    assert_eq!(direct, value);
    assert_eq!(value.get("value"), Some(&json!("<literal> &copy;")));
    if substitutions.is_empty() {
        assert!(value.get("substitutions").is_none());
    } else {
        assert_eq!(
            value.get("substitutions"),
            Some(&serde_json::to_value(substitutions)?)
        );
    }
    Ok(())
}

#[test]
fn content_metadata_has_the_same_presence_rules_without_its_owner() -> Result<(), TestError> {
    let parsed = parse(
        "[quote,attribution='Author',citetitle='Book',reftext='Label',caption='Prefix']\n____\nBody.\n____",
        &Options::default(),
    )?;
    let owner = parsed.document().blocks.first().ok_or("quote")?;
    let mut metadata = owner.metadata().ok_or("metadata")?.clone();
    let direct = serde_json::to_value(&metadata)?;
    assert_eq!(serde_json::to_value(owner)?.get("metadata"), Some(&direct));
    for name in ["attribution", "citetitle", "reftext", "caption"] {
        assert!(
            direct
                .get(name)
                .and_then(Value::as_array)
                .is_some_and(|nodes| !nodes.is_empty())
        );
        assert!(direct.pointer(&format!("/attributes/{name}")).is_none());
    }
    metadata.attribution = Some(ContentAttribute::default());
    metadata.citetitle = Some(ContentAttribute::default());
    metadata.reftext = Some(ContentAttribute::default());
    metadata.caption = Some(acdc_parser::Caption::CustomContent(
        ContentAttribute::default(),
    ));
    let empty = serde_json::to_value(&metadata)?;
    for name in ["attribution", "citetitle", "reftext", "caption"] {
        assert_eq!(empty.get(name), Some(&json!([])));
    }
    metadata.attribution = None;
    metadata.citetitle = None;
    metadata.reftext = None;
    metadata.caption = None;
    let absent = serde_json::to_value(&metadata)?;
    for name in ["attribution", "citetitle", "reftext", "caption"] {
        assert!(absent.get(name).is_none());
    }
    Ok(())
}
