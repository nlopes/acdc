//! Automatic reference results retain catalog content without retaining occurrence controls.

use acdc_converters_core::{
    InlineTextTransform,
    xref::{XrefDisplay, XrefGuard, resolve_xref},
};
use acdc_parser::{
    ContentAttribute, CrossReference, Location, Options, XrefCaptionLabel, XrefStyle,
    ast::CrossReferenceRef, parse,
};

type TestError = Box<dyn std::error::Error>;

#[test]
fn resolved_title_outlives_the_occurrence_and_keeps_its_guard() -> Result<(), TestError> {
    let parsed = parse("[#target]\n== A *title*\n\nBody.\n", &Options::default())?;
    let parsed_references = std::collections::HashMap::from(parsed.document().references());
    let reference = parsed_references.get("target");
    let guard = XrefGuard::default();
    let shared = guard.clone();
    let display = {
        let target = String::from("target");
        let occurrence = CrossReference::new(&target, Location::default());
        resolve_xref(reference, CrossReferenceRef::from(&occurrence), &guard)
    };
    let XrefDisplay::Title(content, scope) = display else {
        return Err(format!("expected title, got {display:?}").into());
    };
    assert_eq!(InlineTextTransform::default().to_string(content), "A title");
    assert!(shared.is_resolving());
    let nested = CrossReference::new("target", Location::default());
    assert!(matches!(
        resolve_xref(reference, CrossReferenceRef::from(&nested), &shared),
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
    let reference = parsed_references.get("target");
    let guard = XrefGuard::default();
    let display = {
        let label = String::from("Plate");
        let mut occurrence = CrossReference::new("target", Location::default());
        occurrence.xrefstyle = XrefStyle::Short;
        occurrence.caption_label = XrefCaptionLabel::AtReference(&label);
        assert!(matches!(
            resolve_xref(reference, CrossReferenceRef::from(&occurrence), &guard),
            XrefDisplay::ShortCaption(prefix) if prefix == "Plate 1"
        ));
        occurrence.caption_label = XrefCaptionLabel::NumberOnly;
        assert!(matches!(
            resolve_xref(reference, CrossReferenceRef::from(&occurrence), &guard),
            XrefDisplay::ShortCaption(prefix) if prefix == "1"
        ));
        occurrence.caption_label = XrefCaptionLabel::AtReference(&label);
        occurrence.xrefstyle = XrefStyle::Full;
        resolve_xref(reference, CrossReferenceRef::from(&occurrence), &guard)
    };
    let XrefDisplay::FullCaption(prefix, content, scope) = display else {
        return Err(format!("expected full caption, got {display:?}").into());
    };
    assert_eq!(prefix, "Plate 1");
    assert_eq!(InlineTextTransform::default().to_string(content), "Photo");
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
    reference.xreflabel = Some(ContentAttribute::default());
    let guard = XrefGuard::default();
    let mut occurrence = CrossReference::new("target", Location::default());
    let display = resolve_xref(
        Some(&reference),
        CrossReferenceRef::from(&occurrence),
        &guard,
    );
    let XrefDisplay::Label(content, scope) = display else {
        return Err(format!("expected present empty label, got {display:?}").into());
    };
    assert_eq!(InlineTextTransform::default().to_string(content), "");
    drop(scope);
    occurrence.target_is_local = false;
    assert!(matches!(
        resolve_xref(Some(&reference), CrossReferenceRef::from(&occurrence), &guard),
        XrefDisplay::External(target) if target == "target"
    ));
    assert!(!guard.is_resolving());
    Ok(())
}
