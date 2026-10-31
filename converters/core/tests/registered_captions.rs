//! Registered caption display preserves prefix and recursion policy.

use acdc_converters_core::xref::{XrefDisplay, XrefGuard, resolve_xref};
use acdc_parser::{
    Caption, CompatibilityMode, ContentAttribute, CrossReference, Location, Options, XrefStyle,
    parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn registered_caption_content_keeps_formatting_policy_and_recursion_guard() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            "[#target]\n.Title\n[caption='*Préfixe* ->. ']\n====\nBody.\n====\n",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let reference = parsed.document().references.get("target");
        let guard = XrefGuard::default();
        let mut xref = CrossReference::new("target", Location::default());
        xref.xrefstyle = XrefStyle::Short;
        assert!(matches!(
            resolve_xref(reference, &xref, &guard),
            XrefDisplay::ShortCaption(prefix) if prefix == "Préfixe →"
        ));
        assert!(!guard.is_resolving());
        xref.xrefstyle = XrefStyle::Full;
        let display = resolve_xref(reference, &xref, &guard);
        assert!(
            matches!(&display, XrefDisplay::FullCaption(prefix, _, _) if prefix == "Préfixe →")
        );
        assert!(matches!(
            resolve_xref(reference, &xref, &guard),
            XrefDisplay::Nested(text) if text == "[target]"
        ));
        drop(display);
        assert!(!guard.is_resolving());
    }
    Ok(())
}

#[test]
fn empty_or_unnumbered_registered_captions_keep_title_fallback() -> Result<(), Error> {
    let parsed = parse(
        "[#target]\n.Title\nimage::photo.png[]\n",
        &Options::default(),
    )?;
    let mut reference = parsed
        .document()
        .references
        .get("target")
        .ok_or("target")?
        .clone();
    let mut xref = CrossReference::new("target", Location::default());
    xref.xrefstyle = XrefStyle::Short;
    let guard = XrefGuard::default();
    for caption in [
        None,
        Some(Caption::Unnumbered),
        Some(Caption::Custom("".into())),
        Some(Caption::CustomContent(ContentAttribute::default())),
        Some(Caption::Numbered {
            kind: acdc_parser::CaptionKind::Figure,
            label: "Figure".into(),
            number: None,
        }),
    ] {
        reference.caption = caption;
        assert!(matches!(
            resolve_xref(Some(&reference), &xref, &guard),
            XrefDisplay::Title(..)
        ));
        assert!(!guard.is_resolving());
    }
    Ok(())
}
