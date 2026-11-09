//! Reference normalization preserves destinations, labels and unsupported controls.
#![cfg(feature = "asg")]

use acdc_parser::{
    AttributeValue, CompatibilityMode, ContentAttribute, CrossReference, InlineMacro, InlineNode,
    Link, Location, Options, Source, XrefSignifier, XrefStyle, asg,
    ast::{BlockKind, BlockRef, ContentRef},
    parse, parse_inline,
};
use serde_json::{Value, json};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn text(value: &Value) -> String {
    match value.get("value").and_then(Value::as_str) {
        Some(text) => text.into(),
        None => value
            .get("inlines")
            .and_then(Value::as_array)
            .map(|nodes| nodes.iter().map(text).collect())
            .unwrap_or_default(),
    }
}

#[test]
fn reference_forms_preserve_targets_and_effective_labels() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (source, variant, target, label) in [
            (
                "link:page.html[Read *this*]",
                "link",
                "page.html",
                "Read this",
            ),
            (
                "https://example.org[Read _this_]",
                "link",
                "https://example.org",
                "Read this",
            ),
            (
                "https://example.org",
                "link",
                "https://example.org",
                "https://example.org",
            ),
            (
                "<https://example.org>",
                "link",
                "https://example.org",
                "https://example.org",
            ),
            (
                "person@example.org",
                "link",
                "mailto:person@example.org",
                "person@example.org",
            ),
            (
                "mailto:person@example.org[]",
                "link",
                "mailto:person@example.org",
                "person@example.org",
            ),
            (
                "link:mailto:person@example.org[]",
                "link",
                "mailto:person@example.org",
                "mailto:person@example.org",
            ),
            ("<<target,*Label*>>", "xref", "target", "Label"),
            (
                "xref:other.adoc#part[_Label_]",
                "xref",
                "other.adoc#part",
                "Label",
            ),
            ("<<target>>", "xref", "target", ""),
        ] {
            let parsed = parse_inline(source, &options)?;
            let before = serde_json::to_value(parsed.content())?;
            let value = asg::export_inlines(parsed.content())?;
            let reference = value
                .as_array()
                .and_then(|nodes| nodes.first())
                .ok_or("missing reference")?;
            assert_eq!(reference.get("name"), Some(&json!("ref")), "{source}");
            assert_eq!(reference.get("variant"), Some(&json!(variant)), "{source}");
            assert_eq!(reference.get("target"), Some(&json!(target)), "{source}");
            assert_eq!(text(reference), label, "{mode:?}: {source}");
            assert!(reference.get("inlines").is_some_and(Value::is_array));
            assert_eq!(serde_json::to_value(parsed.content())?, before);
        }
    }
    Ok(())
}

#[test]
fn formatted_and_nested_labels_remain_inline_trees() -> Result<(), Error> {
    let parsed = parse_inline(
        "link:page.html[Read *bold _nested_* &amp;]",
        &Options::default(),
    )?;
    let value = asg::export_inlines(parsed.content())?;
    assert_eq!(
        value.pointer("/0/inlines/1/variant"),
        Some(&json!("strong"))
    );
    assert_eq!(
        value.pointer("/0/inlines/1/inlines/1/variant"),
        Some(&json!("emphasis"))
    );
    assert_eq!(value.pointer("/0/inlines/3/name"), Some(&json!("charref")));
    let parsed = parse_inline(
        "link:page.html[Before image:photo.png[Alt] after]",
        &Options::default(),
    )?;
    assert!(
        matches!(asg::export_inlines(parsed.content()), Err(asg::Error::Unsupported {path, construct: Some("inline image"), ..})
        if path == "/0/inlines/1")
    );
    Ok(())
}

#[test]
fn bracketed_email_keeps_brackets_outside_the_reference() -> Result<(), Error> {
    let parsed = parse_inline("<person@example.org>", &Options::default())?;
    let value = asg::export_inlines(parsed.content())?;
    assert_eq!(value.as_array().ok_or("array")?.len(), 3);
    assert_eq!(value.pointer("/0/value"), Some(&json!("<")));
    assert_eq!(value.pointer("/1/name"), Some(&json!("ref")));
    assert_eq!(
        value.pointer("/1/inlines/0/value"),
        Some(&json!("person@example.org"))
    );
    assert_eq!(value.pointer("/2/value"), Some(&json!(">")));
    assert!(value.pointer("/0/location").is_none());
    assert!(value.pointer("/1/inlines/0/location").is_none());
    Ok(())
}

#[test]
fn automatic_xrefs_keep_empty_labels_and_exported_target_content() -> Result<(), Error> {
    let parsed = parse(
        "[#target,reftext='Read *this*']\n== Heading\n\nSee <<target>>.",
        &Options::default(),
    )?;
    let value = asg::export_document(parsed.document())?;
    assert_eq!(value.pointer("/blocks/0/id"), Some(&json!("target")));
    assert_eq!(
        value.pointer("/blocks/0/reftext/1/variant"),
        Some(&json!("strong"))
    );
    assert_eq!(
        value.pointer("/blocks/0/blocks/0/inlines/1/target"),
        Some(&json!("target"))
    );
    assert_eq!(
        value.pointer("/blocks/0/blocks/0/inlines/1/inlines"),
        Some(&json!([]))
    );
    Ok(())
}

#[test]
fn mailto_queries_keep_unrelated_headers_and_encode_values() -> Result<(), Error> {
    for (source, expected) in [
        (
            "mailto:a@example.org[Mail,Hello world,First & second]",
            "mailto:a@example.org?subject=Hello%20world&body=First%20%26%20second",
        ),
        (
            "mailto:a@example.org?cc=b@example.org&subject=Old[Mail,New,Body]",
            "mailto:a@example.org?cc=b@example.org&subject=New&body=Body",
        ),
        (
            "mailto:a@example.org?body=Keep[Mail,New]",
            "mailto:a@example.org?body=Keep&subject=New",
        ),
        (
            "mailto:a@example.org[Mail,\"\",\"\"]",
            "mailto:a@example.org?subject=&body=",
        ),
        (
            "mailto:a@example.org[Mail,Café,雪]",
            "mailto:a@example.org?subject=Caf%C3%A9&body=%E9%9B%AA",
        ),
    ] {
        let parsed = parse_inline(source, &Options::default())?;
        let value = asg::export_inlines(parsed.content())?;
        assert_eq!(
            value.pointer("/0/target"),
            Some(&json!(expected)),
            "{source}"
        );
    }
    let mut mail = acdc_parser::Mailto::new(Source::Name("a@example.org"), Location::default());
    mail.replace_subject(Some("Subject"));
    mail.replace_body(Some("First\nsecond\rthird\r\nfourth"));
    let value = asg::export_inlines(ContentRef::from(&ContentAttribute::new(vec![
        InlineNode::Macro(InlineMacro::Mailto(Box::new(mail))),
    ])))?;
    assert_eq!(
        value.pointer("/0/target"),
        Some(&json!(
            "mailto:a@example.org?subject=Subject&body=First%0D%0Asecond%0D%0Athird%0D%0Afourth"
        ))
    );
    Ok(())
}

#[test]
fn scheme_hiding_changes_only_automatic_display_text() -> Result<(), Error> {
    let parsed = parse(
        "= Guide\n:hide-uri-scheme:\n\nhttps://example.org[] and link:https://example.org[Explicit]",
        &Options::default(),
    )?;
    let Some(BlockKind::Paragraph(paragraph)) =
        parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("paragraph".into());
    };
    let value = asg::export_inlines(paragraph.content())?;
    assert_eq!(
        value.pointer("/0/target"),
        Some(&json!("https://example.org"))
    );
    assert_eq!(
        value.pointer("/0/inlines/0/value"),
        Some(&json!("example.org"))
    );
    assert_eq!(
        value.pointer("/2/inlines/0/value"),
        Some(&json!("Explicit"))
    );
    Ok(())
}

#[test]
fn unsupported_reference_properties_fail_at_the_property() -> Result<(), Error> {
    for (source, suffix) in [
        ("link:page.html[Label,title=Tip]", "tooltip"),
        (r#"link:page.html[Label,title=""]"#, "tooltip"),
        ("link:page.html[Label,id=link-id]", "metadata/id"),
        ("link:page.html[Label,role=external]", "metadata/roles"),
        ("link:page.html[Label,opts=nofollow]", "metadata/options"),
        (
            "link:page.html[Label,window=_blank]",
            "metadata/attributes/window",
        ),
        ("xref:target[xrefstyle=short]", "xrefstyle"),
    ] {
        let parsed = parse_inline(source, &Options::default())?;
        assert!(
            matches!(asg::export_inlines(parsed.content()), Err(asg::Error::Unsupported {path, ..})
            if path == format!("/0/{suffix}")),
            "{source}"
        );
    }
    let mut link = Link::new(Source::Name("target"), Location::default());
    link.set_attribute("custom/~".into(), AttributeValue::Bool(false))?;
    assert!(
        matches!(asg::export_inlines(ContentRef::from(&ContentAttribute::new(vec![InlineNode::Macro(InlineMacro::Link(Box::new(link)))]))), Err(asg::Error::Unsupported {path, ..})
        if path == "/0/metadata/attributes/custom~1~0")
    );
    let mut xref = CrossReference::new("target", Location::default());
    xref.set_signifier(XrefSignifier::Omitted);
    assert!(
        matches!(asg::export_inlines(ContentRef::from(&ContentAttribute::new(vec![InlineNode::Macro(InlineMacro::CrossReference(Box::new(xref)))]))), Err(asg::Error::Unsupported {path, ..})
        if path == "/0/signifier")
    );
    Ok(())
}

#[test]
fn target_classification_cannot_be_silently_changed() {
    let mut xref = CrossReference::new("file.adoc", Location::default());
    xref.set_target_is_local(true);
    xref.set_xrefstyle(XrefStyle::Default);
    assert!(
        matches!(asg::export_inlines(ContentRef::from(&ContentAttribute::new(vec![InlineNode::Macro(InlineMacro::CrossReference(Box::new(xref)))]))), Err(asg::Error::Unsupported {path, ..})
        if path == "/0/target_is_local")
    );
}

#[test]
fn customized_signifier_snapshots_are_not_mistaken_for_defaults() -> Result<(), Error> {
    let parsed = parse(
        "= Guide\n:section-refsig: Chapter\n\n== Heading\n\nSee <<_heading>>.",
        &Options::default(),
    )?;
    assert!(
        matches!(asg::export_document(parsed.document()), Err(asg::Error::Unsupported {path, ..})
        if path == "/blocks/0/blocks/0/inlines/1/signifier")
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn non_utf8_reference_targets_are_not_lossily_stringified() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};
    let source = Source::Path(PathBuf::from(OsString::from_vec(vec![b'x', 0xff])));
    let link = Link::new(source, Location::default());
    assert!(
        matches!(asg::export_inlines(ContentRef::from(&ContentAttribute::new(vec![InlineNode::Macro(InlineMacro::Link(Box::new(link)))]))),
        Err(asg::Error::Unsupported {path, ..}) if path == "/0/target")
    );
}

#[test]
fn email_bracket_expansion_preserves_error_indices() -> Result<(), Error> {
    let parsed = parse_inline(
        "<person@example.org> image:photo.png[Alt]",
        &Options::default(),
    )?;
    assert!(
        matches!(asg::export_inlines(parsed.content()), Err(asg::Error::Unsupported {path, ..})
        if path == "/4")
    );
    Ok(())
}
