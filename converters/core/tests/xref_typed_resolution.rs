//! Typed and owner xref results share content, source spans, precedence, and guard behavior.

use acdc_converters_core::xref::{XrefDisplay, XrefGuard, resolve_reference, resolve_xref};
use acdc_parser::{
    CompatibilityMode, ContentAttribute, CrossReference, Location, Options, Title,
    XrefCaptionLabel, XrefSignifier, XrefStyle,
    ast::{ContentRef, CrossReferenceRef, InlineKind, ReferenceRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

const SOURCE: &str = r#"= Book
:doctype: book

[#plain]
== Plain

:sectnums:

[#chapter]
== A *chapter*

[#section]
=== A _section_

[#labelled,reftext='A *label*']
.Labelled title
====
Body.
====

[#untitled]
Untitled paragraph.

[#figure]
.A figure
image::bird.svg[]

[#custom,caption="Plate. "]
.Custom title
====
Body.
====

[appendix]
[#appendix]
== An appendix
"#;

fn assert_content(owner: &[acdc_parser::InlineNode<'_>], typed: ContentRef<'_>) {
    let expected = ContentAttribute::new(owner.to_vec());
    assert_eq!(typed, ContentRef::from(&expected));
}

fn assert_display(
    owner: XrefDisplay<'_, &[acdc_parser::InlineNode<'_>]>,
    typed: XrefDisplay<'_, ContentRef<'_>>,
) -> Result<(), Error> {
    match (owner, typed) {
        (XrefDisplay::Title(a, _), XrefDisplay::Title(b, _))
        | (XrefDisplay::Label(a, _), XrefDisplay::Label(b, _)) => assert_content(a, b),
        (XrefDisplay::FullCaption(a, ac, _), XrefDisplay::FullCaption(b, bc, _)) => {
            assert_eq!(a, b);
            assert_content(ac, bc);
        }
        (XrefDisplay::Emphasized(a, ac, _), XrefDisplay::Emphasized(b, bc, _)) => {
            assert_eq!(a, b);
            assert_content(ac, bc);
        }
        (XrefDisplay::ShortCaption(a), XrefDisplay::ShortCaption(b))
        | (XrefDisplay::External(a), XrefDisplay::External(b))
        | (XrefDisplay::Fallback(a), XrefDisplay::Fallback(b))
        | (XrefDisplay::Unresolved(a), XrefDisplay::Unresolved(b))
        | (XrefDisplay::Nested(a), XrefDisplay::Nested(b)) => assert_eq!(a, b),
        (a, b) => return Err(format!("different xref forms: {a:?} vs {b:?}").into()),
    }
    Ok(())
}

#[test]
fn typed_xrefs_keep_owner_results_and_guard_lifetimes() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        check_mode(mode)?;
    }
    Ok(())
}

fn check_mode(mode: CompatibilityMode) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let document = &acdc_parser::Document::from(parsed.document());
    let outer = CrossReference::new("labelled", Location::default());
    let outer_reference = document.references.get("labelled").ok_or("labelled")?;
    for target in [
        "plain",
        "chapter",
        "section",
        "labelled",
        "untitled",
        "figure",
        "custom",
        "appendix",
        "missing",
        "other.adoc#part",
        "",
    ] {
        for style in [
            XrefStyle::Default,
            XrefStyle::Basic,
            XrefStyle::Short,
            XrefStyle::Full,
        ] {
            for (local, nested) in [(true, false), (false, false), (true, true)] {
                let owner_guard = XrefGuard::default();
                let shared_owner_guard = owner_guard.clone();
                let independent_guard = XrefGuard::default();
                let owner_outer =
                    nested.then(|| resolve_xref(Some(outer_reference), &outer, &owner_guard));
                let typed_outer = nested.then(|| {
                    resolve_reference(
                        Some(ReferenceRef::from(outer_reference)),
                        &outer,
                        &independent_guard,
                    )
                });
                let mut xref = CrossReference::new(target, Location::default());
                xref.xrefstyle = style;
                xref.target_is_local = local;
                let reference = document.references.get(target);
                let owner = resolve_xref(reference, &xref, &owner_guard);
                let typed = resolve_reference(
                    reference.map(ReferenceRef::from),
                    CrossReferenceRef::from(&xref),
                    &independent_guard,
                );
                assert_eq!(
                    shared_owner_guard.is_resolving(),
                    independent_guard.is_resolving()
                );
                assert_display(owner, typed)?;
                assert_eq!(owner_guard.is_resolving(), nested);
                assert_eq!(independent_guard.is_resolving(), nested);
                drop((owner_outer, typed_outer));
                assert!(!shared_owner_guard.is_resolving());
                assert!(!independent_guard.is_resolving());
            }
        }
    }
    Ok(())
}

#[test]
fn typed_xrefs_preserve_empty_titles_labels_and_temporary_overrides() -> Result<(), Error> {
    let parsed = parse(SOURCE, &Options::default())?;
    let parsed_references = std::collections::HashMap::from(parsed.document().references());
    let original = parsed_references.get("figure").ok_or("figure")?;
    for title in [
        None,
        Some(Title::default()),
        Some(Title::new(Vec::new())),
        original.title.clone(),
    ] {
        for label in [None, Some(ContentAttribute::default())] {
            let mut reference = original.clone();
            reference.title = title.clone();
            reference.xreflabel = label;
            let guard = XrefGuard::default();
            let other_guard = XrefGuard::default();
            let owner = {
                let caption = String::from("Plate");
                let signifier = String::from("Unit");
                let mut xref = CrossReference::new("figure", Location::default());
                xref.xrefstyle = XrefStyle::Full;
                xref.caption_label = XrefCaptionLabel::AtReference(&caption);
                xref.set_signifier(XrefSignifier::AtReference(&signifier));
                resolve_xref(Some(&reference), &xref, &guard)
            };
            let typed = {
                let caption = String::from("Plate");
                let signifier = String::from("Unit");
                let mut xref = CrossReference::new("figure", Location::default());
                xref.xrefstyle = XrefStyle::Full;
                xref.caption_label = XrefCaptionLabel::AtReference(&caption);
                xref.set_signifier(XrefSignifier::AtReference(&signifier));
                resolve_reference(Some(ReferenceRef::from(&reference)), &xref, &other_guard)
            };
            assert_display(owner, typed)?;
            assert!(!guard.is_resolving());
            assert!(!other_guard.is_resolving());
        }
    }
    Ok(())
}

#[test]
fn typed_xref_label_borrows_the_catalog_text() -> Result<(), Error> {
    let parsed = parse("[[target,A label]]Text.\n", &Options::default())?;
    let reference = parsed
        .document()
        .references()
        .get("target")
        .ok_or("target")?;
    let label = reference.xreflabel().ok_or("label")?;
    let Some(first) = label.inlines().next() else {
        return Err("empty label".into());
    };
    let InlineKind::Plain(expected) = first.kind() else {
        return Err("plain label".into());
    };
    let guard = XrefGuard::default();
    let display = {
        let target = String::from("target");
        let xref = CrossReference::new(&target, Location::default());
        resolve_reference(Some(reference), &xref, &guard)
    };
    let XrefDisplay::Label(content, scope) = display else {
        return Err("label result".into());
    };
    let Some(node) = content.inlines().next() else {
        return Err("label node".into());
    };
    let InlineKind::Plain(actual) = node.kind() else {
        return Err("plain result".into());
    };
    assert_eq!(actual.text(), expected.text());
    assert!(std::ptr::eq(
        actual.text().as_ptr(),
        expected.text().as_ptr()
    ));
    assert!(guard.is_resolving());
    drop(scope);
    assert!(!guard.is_resolving());
    Ok(())
}
