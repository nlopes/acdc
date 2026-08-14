use acdc_parser::{AttributeValue, Block, Caption, CaptionKind, InlineNode, Options, parse};

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
        let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
            return Err(format!("expected a paragraph for [{attributes}]").into());
        };

        assert_eq!(paragraph.metadata.style, attributes.split(',').next());
        assert!(matches!(
            paragraph.content.as_slice(),
            [InlineNode::PlainText(plain)] if plain.content == "image::sample.png[Alt]"
        ));

        match (paragraph.metadata.caption.as_ref(), caption_kind) {
            (
                Some(Caption::Numbered {
                    kind,
                    number: Some(number),
                    ..
                }),
                Some(expected_kind),
            ) => {
                assert_eq!(*kind, expected_kind);
                assert_eq!(number.get(), 1);
            }
            (None | Some(Caption::Unnumbered), None) => {}
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
        let Some(Block::Image(image)) = parsed.document().blocks.first() else {
            return Err(format!("expected an image for [{style}]").into());
        };

        assert_eq!(
            image.metadata.attributes.get("alt"),
            Some(&AttributeValue::String("Alt".into()))
        );
        assert!(matches!(
            image.metadata.caption.as_ref(),
            Some(Caption::Numbered {
                kind: CaptionKind::Figure,
                number: Some(number),
                ..
            }) if number.get() == 1
        ));
    }

    Ok(())
}
