use acdc_parser::{
    AttributeValue, Caption, CaptionKind, DocumentAttributes, Location, Options, Paragraph,
    ast::{CaptionRef, ParagraphRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn resolve<'a>(
    paragraph: &Paragraph<'a>,
    attributes: &DocumentAttributes<'_>,
    fallback: Option<CaptionKind>,
) -> Option<Caption<'a>> {
    let result = paragraph.metadata.caption.clone().or_else(|| {
        fallback.map(|kind| Caption::resolve_owned(&paragraph.metadata, attributes, kind))
    });
    assert_eq!(
        CaptionRef::resolve(
            ParagraphRef::from(paragraph).attributes(),
            attributes.into(),
            fallback
        ),
        result.as_ref().map(CaptionRef::from)
    );
    result
}

#[test]
fn fallback_captions_preserve_precedence_presence_and_exact_label_text() -> Result<(), Error> {
    for (entries, label) in [
        ("", Some("Figure")),
        (":figure-caption!:\n", None),
        (":figure-caption:\n", Some("")),
        (":figure-caption: 'Figure'\n", Some("'Figure'")),
    ] {
        for global in [None, Some(""), Some("Global:")] {
            let global_entry = global.map_or(String::new(), |value| format!(":caption: {value}\n"));
            let input = format!("= Doc\n{entries}{global_entry}\nText.");
            let parsed = parse(&input, &Options::default())?;
            let attributes = DocumentAttributes::from(parsed.document().attributes());
            for local in [
                None,
                Some(AttributeValue::None),
                Some(AttributeValue::Bool(false)),
                Some(AttributeValue::from("")),
                Some(AttributeValue::from("Local:")),
            ] {
                let expected_local = match &local {
                    Some(AttributeValue::String(value)) => Some(value.as_ref()),
                    _ => None,
                };
                let mut paragraph = Paragraph::new(Vec::new(), Location::default());
                if let Some(local) = &local {
                    paragraph
                        .metadata
                        .attributes
                        .set("caption".into(), local.clone());
                }
                let resolved =
                    resolve(&paragraph, &attributes, Some(CaptionKind::Figure)).ok_or("caption")?;
                let expected = if let Some(prefix) = expected_local.or(global) {
                    CaptionRef::Custom(prefix)
                } else if let Some(label) = label {
                    CaptionRef::Numbered {
                        kind: CaptionKind::Figure,
                        label,
                        number: None,
                    }
                } else {
                    CaptionRef::Unnumbered
                };
                assert_eq!(
                    CaptionRef::from(&resolved),
                    expected,
                    "{entries:?} {global:?} {local:?}"
                );
                assert!(resolve(&paragraph, &attributes, None).is_none());
            }
        }
    }
    Ok(())
}

#[test]
fn only_collapsible_examples_suppress_the_fallback_prefix() -> Result<(), Error> {
    let parsed = parse("= Doc\n:caption: Global:\n\nText.", &Options::default())?;
    let attributes = DocumentAttributes::from(parsed.document().attributes());
    let mut paragraph = Paragraph::new(Vec::new(), Location::default());
    paragraph.metadata.options.push("collapsible");
    paragraph
        .metadata
        .attributes
        .set("caption".into(), "Local:".into());
    for kind in [
        CaptionKind::Example,
        CaptionKind::Figure,
        CaptionKind::Listing,
        CaptionKind::Table,
    ] {
        let resolved = resolve(&paragraph, &attributes, Some(kind)).ok_or("caption")?;
        assert_eq!(
            CaptionRef::from(&resolved),
            CaptionRef::Custom(if kind == CaptionKind::Example {
                ""
            } else {
                "Local:"
            })
        );
    }
    Ok(())
}

#[test]
fn fallback_labels_borrow_the_supplied_snapshot_and_see_current_owner_edits() -> Result<(), Error> {
    let parsed = parse(
        "= Doc\n:figure-caption: 'Plate'\n\nText.",
        &Options::default(),
    )?;
    let mut paragraph = Paragraph::new(Vec::new(), Location::default());
    let caption = {
        CaptionRef::resolve(
            ParagraphRef::from(&paragraph).attributes(),
            parsed.document().attributes(),
            Some(CaptionKind::Figure),
        )
        .ok_or("caption")?
    };
    let CaptionRef::Numbered {
        label,
        number: None,
        ..
    } = caption
    else {
        return Err("numbered caption".into());
    };
    assert_eq!(label, "'Plate'");
    assert!(std::ptr::eq(
        label,
        parsed
            .document()
            .attributes()
            .get("figure-caption")
            .and_then(|value| value.text())
            .ok_or("label")?
    ));
    paragraph
        .metadata
        .attributes
        .set("caption".into(), String::from("Local:").into());
    let caption = CaptionRef::resolve(
        ParagraphRef::from(&paragraph).attributes(),
        parsed.document().attributes(),
        Some(CaptionKind::Figure),
    );
    assert_eq!(caption, Some(CaptionRef::Custom("Local:")));
    paragraph.metadata.caption = Some(Caption::Unnumbered);
    assert_eq!(
        CaptionRef::resolve(
            ParagraphRef::from(&paragraph).attributes(),
            parsed.document().attributes(),
            Some(CaptionKind::Figure)
        ),
        Some(CaptionRef::Unnumbered)
    );
    Ok(())
}

#[test]
fn unsupported_elements_do_not_gain_fallback_captions() -> Result<(), Error> {
    use acdc_parser::ast::{BlockKind, InlineKind};
    let parsed = parse("'''\n\nimage:bird.svg[]\n", &Options::default())?;
    let mut blocks = parsed.document().blocks();
    let block = blocks.next().ok_or("thematic break")?;
    assert!(matches!(block.kind(), BlockKind::ThematicBreak(_)));
    assert_eq!(
        CaptionRef::resolve(
            block.attributes().ok_or("attributes")?,
            parsed.document().attributes(),
            Some(CaptionKind::Figure)
        ),
        None
    );
    let BlockKind::Paragraph(paragraph) = blocks.next().ok_or("paragraph")?.kind() else {
        return Err("paragraph".into());
    };
    let InlineKind::Image(image) = paragraph.content().inlines().next().ok_or("image")?.kind()
    else {
        return Err("image".into());
    };
    assert_eq!(
        CaptionRef::resolve(
            image.attributes(),
            parsed.document().attributes(),
            Some(CaptionKind::Figure)
        ),
        None
    );
    Ok(())
}
