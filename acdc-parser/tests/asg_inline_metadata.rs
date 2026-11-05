#![cfg(feature = "asg")]

use acdc_parser::{
    Bold, CompatibilityMode, ContentAttribute, Document, ElementId, Form, Highlight, InlineMacro,
    InlineNode, Link, Location, Options, Source, asg, ast::ContentRef, parse, parse_file,
    parse_inline,
};
use serde_json::{Value, json};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn extended(content: &ContentAttribute<'_>) -> Result<Value, asg::Error> {
    asg::inlines_with_profile(ContentRef::from(content), asg::Profile::Acdc)
}

#[test]
fn source_metadata_survives_checked_export_without_changing_native_values() -> Result<(), Error> {
    let source = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/asg/inline_metadata.adoc"
    );
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = parse_file(source, &options)?;
        let native = serde_json::to_value(parsed.document())?;
        let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        let expected: Value = serde_json::from_str(&std::fs::read_to_string(
            std::path::Path::new(source).with_extension("json"),
        )?)?;
        assert_eq!(value, expected, "{mode:?}");
        assert_eq!(serde_json::to_value(parsed.document())?, native);
        assert_eq!(
            asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?,
            value
        );
        let text = serde_json::to_string(&value)?;
        for expected in [
            "\"id\":\"strong\"",
            "\"variant\":\"generic\"",
            "\"tooltip\":[]",
            "\"window\":\"_blank\"",
            "\"target\":\"mailto:person@example.org?subject=Subject&body=Body\"",
        ] {
            assert!(text.contains(expected), "{expected}: {text}");
        }
        assert!(
            matches!(asg::document(parsed.document()), Err(asg::Error::Unsupported { path, .. }) if path.ends_with("/id"))
        );
    }
    Ok(())
}

#[test]
fn empty_span_roles_preserve_generic_highlights_without_inventing_controls() -> Result<(), Error> {
    for role in [None, Some(""), Some(" \t"), Some("one one\u{a0}two")] {
        let mut span = Highlight::new(Vec::new(), Form::Constrained, Location::default());
        span.replace_role(role);
        let content = ContentAttribute::new(vec![InlineNode::HighlightText(span)]);
        let value = extended(&content)?;
        assert_eq!(
            value.pointer("/0/variant"),
            Some(&json!(if role.is_some() { "generic" } else { "mark" }))
        );
        let tokens: Vec<_> = role.unwrap_or_default().split_whitespace().collect();
        assert_eq!(
            value
                .get(0)
                .ok_or("inline node")?
                .pointer("/metadata/roles"),
            (!tokens.is_empty()).then(|| json!(tokens)).as_ref()
        );
        assert!(
            value
                .get(0)
                .ok_or("inline node")?
                .pointer("/metadata/options")
                .is_none()
        );
        assert_eq!(
            asg::inlines(ContentRef::from(&content)).is_err(),
            role.is_some()
        );
    }
    let mut span = Bold::new(Vec::new(), Form::Unconstrained, Location::default());
    span.set_id(Some(ElementId::new("", Location::default())));
    let value = extended(&ContentAttribute::new(vec![InlineNode::BoldText(span)]))?;
    assert_eq!(value.pointer("/0/id"), Some(&json!("")));
    assert_eq!(value.pointer("/0/form"), Some(&json!("unconstrained")));
    Ok(())
}

#[test]
fn nested_unsupported_values_still_report_their_own_path() -> Result<(), Error> {
    let parsed = parse(
        "link:https://example.org[Label,title='stem:[x]']",
        &Options::default(),
    )?;
    assert!(
        matches!(asg::document_with_profile(parsed.document(), asg::Profile::Acdc), Err(asg::Error::Unsupported { path, .. }) if path.contains("/tooltip/"))
    );
    let mut link = Link::new(Source::Name("target"), Location::default());
    link.set_attribute("literal".into(), "\"keep\"".to_owned().into())?;
    let value = extended(&ContentAttribute::new(vec![InlineNode::Macro(
        InlineMacro::Link(Box::new(link)),
    )]))?;
    assert_eq!(
        value
            .get(0)
            .ok_or("inline node")?
            .pointer("/metadata/attributes/literal"),
        Some(&json!("\"keep\""))
    );
    Ok(())
}

#[test]
fn inline_metadata_schema_rejects_block_only_and_unknown_fields() -> Result<(), Error> {
    let schema: Value = serde_json::from_str(include_str!("../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    let wrapper = |inline: Value| json!({"name":"document","type":"block","profile":asg::ACDC_SCHEMA_ID,"blocks":[{"name":"paragraph","type":"block","inlines":[inline]}]});
    for valid in [
        json!({"name":"span","type":"inline","variant":"generic","form":"constrained","inlines":[],"id":"x","metadata":{"roles":["one","two"]}}),
        json!({"name":"ref","type":"inline","variant":"link","target":"target","inlines":[],"id":"x","tooltip":[],"metadata":{"options":["custom"],"attributes":{"window":"_blank"}}}),
        json!({"name":"image","type":"inline","target":"image","metadata":{"roles":["one"]}}),
    ] {
        assert!(validator.is_valid(&wrapper(valid.clone())), "{valid}");
        for extra in [
            json!({"style":"source"}),
            json!({"positional_attributes":["value"]}),
            json!({"location":[{"line":1,"col":1},{"line":1,"col":2}]}),
            json!({"extra":true}),
        ] {
            let mut invalid = valid.clone();
            invalid
                .as_object_mut()
                .ok_or("inline object")?
                .insert("metadata".into(), extra);
            assert!(!validator.is_valid(&wrapper(invalid)));
        }
        let mut invalid = valid.clone();
        invalid
            .as_object_mut()
            .ok_or("inline object")?
            .insert("id".into(), json!(2));
        assert!(!validator.is_valid(&wrapper(invalid)));
    }
    for metadata in [
        json!({"options":["custom"]}),
        json!({"attributes":{"custom":"value"}}),
    ] {
        assert!(!validator.is_valid(&wrapper(json!({"name":"span","type":"inline","variant":"generic","form":"constrained","inlines":[],"metadata":metadata}))));
    }
    Ok(())
}

#[test]
fn inline_id_catalogs_keep_duplicate_and_stale_checks() -> Result<(), Error> {
    for source in [
        "[#duplicate]*First* and [#duplicate]_Second_",
        "link:one[One,id=duplicate] link:two[Two,id=duplicate]",
        "[#duplicate]\n== Section\n\nlink:one[One,id=duplicate]",
    ] {
        let parsed = parse(source, &Options::default())?;
        assert!(
            matches!(asg::document_with_profile(parsed.document(), asg::Profile::Acdc), Err(asg::Error::Unsupported { message, .. }) if message.contains("duplicate"))
        );
    }
    let parsed = parse("link:one[One,id=target]", &Options::default())?;
    let mut document = Document::from(parsed.document());
    document.set_blocks(Vec::new());
    assert!(
        matches!(asg::document_with_profile((&document).into(), asg::Profile::Acdc), Err(asg::Error::Unsupported { path, .. }) if path == "/references/target")
    );
    Ok(())
}

#[test]
fn each_supported_span_form_retains_identity_and_roles() -> Result<(), Error> {
    for (source, variant, form) in [
        ("[#s.one.two]*Strong*", "strong", "constrained"),
        ("[#s.one.two]**Strong**", "strong", "unconstrained"),
        ("[#s.one.two]_Emphasis_", "emphasis", "constrained"),
        ("[#s.one.two]__Emphasis__", "emphasis", "unconstrained"),
        ("[#s.one.two]`Code`", "code", "constrained"),
        ("[#s.one.two]``Code``", "code", "unconstrained"),
        ("[#s.one.two]#Span#", "generic", "constrained"),
        ("[#s.one.two]##Span##", "generic", "unconstrained"),
    ] {
        for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
            let options = Options::builder().with_compatibility_mode(mode).build()?;
            let parsed = parse_inline(source, &options)?;
            let value = asg::inlines_with_profile(parsed.content(), asg::Profile::Acdc)?;
            assert_eq!(
                value.pointer("/0/variant"),
                Some(&json!(variant)),
                "{source} {mode:?}"
            );
            assert_eq!(value.pointer("/0/form"), Some(&json!(form)));
            assert_eq!(value.pointer("/0/id"), Some(&json!("s")));
            assert_eq!(
                value.pointer("/0/metadata/roles"),
                Some(&json!(["one", "two"]))
            );
            assert!(
                matches!(asg::inlines(parsed.content()), Err(asg::Error::Unsupported {path, ..}) if path == "/0/id")
            );
        }
    }
    Ok(())
}
