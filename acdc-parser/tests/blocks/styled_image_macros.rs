use acdc_parser::{
    CaptionKind, Options,
    ast::{BlockKind, BlockRef, CaptionRef, InlineKind, InlineRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn verbatim_styles_keep_block_image_macros_literal() -> Result<(), Error> {
    let cases = [
        ("listing", Some(CaptionKind::Listing)),
        ("source,rust", Some(CaptionKind::Listing)),
        ("literal", None),
        ("verse", None),
    ];

    for (attributes, caption_kind) in cases {
        let source = format!(
            "= T\n:listing-caption: Listing\n\n.Title\n[{attributes}]\nimage::sample.png[Alt]\n"
        );
        let parsed = parse(&source, &Options::default())?;
        let Some(BlockKind::Paragraph(paragraph)) =
            parsed.document().blocks().next().map(BlockRef::kind)
        else {
            return Err(format!("expected a paragraph for [{attributes}]").into());
        };

        assert_eq!(paragraph.style(), attributes.split(',').next());
        assert!(matches!(
            &*paragraph.content().inlines().map(InlineRef::kind).collect::<Vec<_>>(),
            [InlineKind::Plain(plain)] if plain.text() == "image::sample.png[Alt]"
        ));

        match (
            CaptionRef::resolve(paragraph.attributes(), parsed.document().attributes(), None),
            caption_kind,
        ) {
            (
                Some(CaptionRef::Numbered {
                    kind,
                    number: Some(number),
                    ..
                }),
                Some(expected_kind),
            ) => {
                assert_eq!(kind, expected_kind);
                assert_eq!(number.get(), 1);
            }
            (None | Some(CaptionRef::Unnumbered), None) => {}
            (actual, expected) => {
                return Err(format!(
                    "unexpected caption for [{attributes}]: {actual:?}, expected {expected:?}"
                )
                .into());
            }
        }
    }

    Ok(())
}

#[test]
fn non_verbatim_styles_still_produce_images() -> Result<(), Error> {
    for style in ["example", "quote", "custom"] {
        let source = format!("= T\n\n.Title\n[{style}]\nimage::sample.png[Alt]\n");
        let parsed = parse(&source, &Options::default())?;
        let Some(BlockKind::Image(image)) = parsed.document().blocks().next().map(BlockRef::kind)
        else {
            return Err(format!("expected an image for [{style}]").into());
        };

        assert!(
            matches!(image.alt().map(|alt| alt.inlines().map(InlineRef::kind).collect::<Vec<_>>()).as_deref(), Some([InlineKind::Plain(text)]) if text.text() == "Alt")
        );
        assert!(
            serde_json::to_value(parsed.document())?
                .pointer("/blocks/0/metadata/attributes/alt")
                .is_none()
        );
        assert!(matches!(
            CaptionRef::resolve(image.attributes(), parsed.document().attributes(), None),
            Some(CaptionRef::Numbered {
                kind: CaptionKind::Figure,
                number: Some(number),
                ..
            }) if number.get() == 1
        ));
    }

    Ok(())
}
