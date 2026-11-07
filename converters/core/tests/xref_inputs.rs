//! Automatic reference results borrow prepared content and retain their resolution guard.

use acdc_converters_core::{
    InlineTextTransform,
    xref::{XrefDisplay, XrefGuard, resolve_reference},
};
use acdc_parser::{
    ContentAttribute, CrossReference, Location, Options, XrefCaptionLabel, XrefStyle,
    ast::CrossReferenceRef, parse,
};

type TestError = Box<dyn std::error::Error>;

#[test]
fn resolved_title_keeps_catalog_content_and_its_guard() -> Result<(), TestError> {
    let parsed = parse("[#target]\n== A *title*\n\nBody.\n", &Options::default())?;
    let parsed_references = std::collections::HashMap::from(parsed.document().references());
    let reference = parsed_references
        .get("target")
        .map(acdc_parser::ast::ReferenceRef::from);
    let guard = XrefGuard::default();
    let shared = guard.clone();
    let target = String::from("target");
    let occurrence = CrossReference::new(&target, Location::default());
    let display = resolve_reference(reference, CrossReferenceRef::from(&occurrence), &guard);
    let XrefDisplay::Title(content, scope) = display else {
        return Err(format!("expected title, got {display:?}").into());
    };
    assert_eq!(
        InlineTextTransform::default().content_to_string(content),
        "A title"
    );
    assert!(shared.is_resolving());
    let nested = CrossReference::new("target", Location::default());
    assert!(matches!(
        resolve_reference(reference, CrossReferenceRef::from(&nested), &shared),
        XrefDisplay::Nested(text) if text == "[target]"
    ));
    drop(scope);
    assert!(!shared.is_resolving());
    Ok(())
}

#[test]
fn caption_prefixes_own_temporary_overrides_and_read_current_controls() -> Result<(), TestError> {
    let parsed = parse(
        "[#target]\n.Photo\nimage::photo.png[]\n",
        &Options::default(),
    )?;
    let parsed_references = std::collections::HashMap::from(parsed.document().references());
    let reference = parsed_references
        .get("target")
        .map(acdc_parser::ast::ReferenceRef::from);
    let guard = XrefGuard::default();
    let label = String::from("Plate");
    let mut occurrence = CrossReference::new("target", Location::default());
    let display = {
        occurrence.set_xrefstyle(XrefStyle::Short);
        occurrence.set_caption_label(XrefCaptionLabel::AtReference(&label));
        assert!(matches!(
            resolve_reference(reference, CrossReferenceRef::from(&occurrence), &guard),
            XrefDisplay::ShortCaption(prefix) if prefix == "Plate 1"
        ));
        occurrence.set_caption_label(XrefCaptionLabel::NumberOnly);
        assert!(matches!(
            resolve_reference(reference, CrossReferenceRef::from(&occurrence), &guard),
            XrefDisplay::ShortCaption(prefix) if prefix == "1"
        ));
        occurrence.set_caption_label(XrefCaptionLabel::AtReference(&label));
        occurrence.set_xrefstyle(XrefStyle::Full);
        resolve_reference(reference, CrossReferenceRef::from(&occurrence), &guard)
    };
    let XrefDisplay::FullCaption(prefix, content, scope) = display else {
        return Err(format!("expected full caption, got {display:?}").into());
    };
    assert_eq!(prefix, "Plate 1");
    assert_eq!(
        InlineTextTransform::default().content_to_string(content),
        "Photo"
    );
    assert!(guard.is_resolving());
    drop(scope);
    assert!(!guard.is_resolving());
    Ok(())
}

#[test]
fn empty_labels_and_external_classification_keep_precedence() -> Result<(), TestError> {
    let parsed = parse("[#target]\n== A title\n\nBody.\n", &Options::default())?;
    let parsed_references = std::collections::HashMap::from(parsed.document().references());
    let mut reference = parsed_references.get("target").ok_or("target")?.clone();
    reference.set_xreflabel(Some(ContentAttribute::default()));
    let guard = XrefGuard::default();
    let mut occurrence = CrossReference::new("target", Location::default());
    let display = resolve_reference(
        Some(acdc_parser::ast::ReferenceRef::from(&reference)),
        CrossReferenceRef::from(&occurrence),
        &guard,
    );
    let XrefDisplay::Label(content, scope) = display else {
        return Err(format!("expected present empty label, got {display:?}").into());
    };
    assert_eq!(
        InlineTextTransform::default().content_to_string(content),
        ""
    );
    drop(scope);
    occurrence.set_target_is_local(false);
    assert!(matches!(
        resolve_reference(Some(acdc_parser::ast::ReferenceRef::from(&reference)), CrossReferenceRef::from(&occurrence), &guard),
        XrefDisplay::External(target) if target == "target"
    ));
    assert!(!guard.is_resolving());
    Ok(())
}
