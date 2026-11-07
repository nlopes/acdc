//! Registered caption display preserves prefix and recursion policy.

use acdc_converters_core::{
    InlineTextTransform,
    xref::{XrefDisplay, XrefGuard, resolve_reference, with_caption_prefix},
};
use acdc_parser::{
    Caption, CompatibilityMode, ContentAttribute, CrossReference, Location, Options, XrefStyle,
    ast::{CaptionRef, ContentRef, InlineKind},
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
        let reference = parsed
            .document()
            .references()
            .get("target")
            .ok_or("target")?;
        let Some(CaptionRef::CustomContent(original)) = reference.caption() else {
            return Err("expected parsed caption".into());
        };
        let before = ContentAttribute::from(original);
        let guard = XrefGuard::default();
        let mut xref = CrossReference::new("target", Location::default());
        for style in [XrefStyle::Short, XrefStyle::Full] {
            xref.set_xrefstyle(style);
            let display = resolve_reference(Some(reference), &xref, &guard);
            let prefix = if let XrefDisplay::ShortCustomCaption(prefix, _) = &display {
                *prefix
            } else if let XrefDisplay::FullCustomCaption(prefix, _, _) = &display {
                *prefix
            } else {
                return Err(format!("expected custom caption, got {display:?}").into());
            };
            assert_eq!(prefix, original);
            with_caption_prefix(prefix, |prefix| {
                assert!(
                    prefix
                        .inlines()
                        .any(|node| matches!(node.kind(), InlineKind::Bold(_)))
                );
                assert_eq!(
                    InlineTextTransform::default().content_to_string(prefix),
                    "Préfixe →"
                );
            });
            assert!(guard.is_resolving());
            assert!(matches!(
                resolve_reference(Some(reference), &xref, &guard),
                XrefDisplay::Nested(text) if text == "[target]"
            ));
            drop(display);
            assert!(!guard.is_resolving());
            assert_eq!(ContentAttribute::from(original), before);
        }
    }
    Ok(())
}

#[test]
fn custom_caption_suffix_does_not_change_formatted_punctuation() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (source, expected) in [
            ("*Prefix*. ", "Prefix"),
            ("*Prefix.* ", "Prefix. "),
            ("**Prefix. **", "Prefix. "),
            ("Prefix. ", "Prefix"),
            ("Prefix. . ", "Prefix. "),
            ("*Prefix* pass:[. ]", "Prefix "),
            ("*Prefix* pass:[.] ", "Prefix "),
            (
                "[.underline]++Prefix. ++",
                if mode == CompatibilityMode::Draft {
                    "[.underline]Prefix"
                } else {
                    "Prefix. "
                },
            ),
            ("*Prefix*.  ", "Prefix.  "),
            ("*Prefix*.π", "Prefix.π"),
            (". ", ""),
        ] {
            let source = format!("[#target,caption='{source}']\n.Title\n====\nBody.\n====\n");
            let parsed = parse(&source, &options)?;
            let reference = parsed
                .document()
                .references()
                .get("target")
                .ok_or("target")?;
            let Some(CaptionRef::CustomContent(prefix)) = reference.caption() else {
                return Err(format!("expected parsed caption: {source}").into());
            };
            let original = ContentAttribute::from(prefix);
            let text = with_caption_prefix(prefix, |prefix| {
                InlineTextTransform::default().content_to_string(prefix)
            });
            assert_eq!(text, expected, "{mode:?}: {source}");
            assert_eq!(prefix, ContentRef::from(&original));
        }
    }
    Ok(())
}

#[test]
fn empty_or_unnumbered_registered_captions_keep_title_fallback() -> Result<(), Error> {
    let parsed = parse(
        "[#target]\n.Title\nimage::photo.png[]\n",
        &Options::default(),
    )?;
    let parsed_references = std::collections::HashMap::from(parsed.document().references());
    let mut reference = parsed_references.get("target").ok_or("target")?.clone();
    let mut xref = CrossReference::new("target", Location::default());
    xref.set_xrefstyle(XrefStyle::Short);
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
        reference.set_caption(caption);
        assert!(matches!(
            resolve_reference(
                Some(acdc_parser::ast::ReferenceRef::from(&reference)),
                &xref,
                &guard
            ),
            XrefDisplay::Title(..)
        ));
        assert!(!guard.is_resolving());
    }
    Ok(())
}
