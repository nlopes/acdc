use crate::{
    Block,
    CaptionKind::{self, Example, Figure, Listing, Table},
    CompatibilityMode, Options,
    ast::{BlockRef, CaptionRef},
    model::Delimited,
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("Text.", [None, Some(Listing), None, Some(Example), Some(Listing), None])]
#[case("====\nBody.\n====", [Some(Example); 6])]
#[case("|===\n|Cell\n|===", [Some(Table); 6])]
#[case("image::bird.svg[]", [Some(Figure); 6])]
#[case("----\nCode.\n----", [Some(Listing), Some(Listing), None, Some(Listing), Some(Listing), Some(Listing)])]
#[case("....\nLiteral.\n....", [None, Some(Listing), None, None, Some(Listing), None])]
#[case("--\nBody.\n--", [None, Some(Listing), None, Some(Example), Some(Listing), None])]
#[case("****\nBody.\n****", [None; 6])]
#[case("____\nWords.\n____", [None; 6])]
#[case("[verse]\n____\nWords.\n____", [None; 6])]
#[case("++++\nRaw.\n++++", [None; 6])]
#[case("[stem]\n++++\nx\n++++", [None; 6])]
#[case("////\nComment.\n////", [None; 6])]
fn caption_context_uses_the_current_form_and_style(
    #[case] body: &str,
    #[case] expected: [Option<CaptionKind>; 6],
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("= Doc\n:listing-caption: Listing\n\n{body}\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut block = parsed
        .document_storage()
        .blocks
        .first()
        .ok_or("block")?
        .clone();
    for (style, expected) in [
        None,
        Some("source"),
        Some("literal"),
        Some("example"),
        Some("listing"),
        Some("other"),
    ]
    .into_iter()
    .zip(expected)
    {
        let metadata = block.metadata_mut().ok_or("block with editable style")?;
        metadata.style = style;
        let before = serde_json::to_value(&block)?;
        assert_eq!(
            CaptionKind::for_block(BlockRef::from(&block)),
            expected,
            "{body:?} {style:?}"
        );
        let copy = Block::from(BlockRef::from(&block));
        assert_eq!(
            CaptionKind::for_block(BlockRef::from(&copy)),
            expected,
            "owner adapter: {body:?} {style:?}"
        );
        assert_eq!(serde_json::to_value(&block)?, before);
    }
    Ok(())
}

#[test]
fn current_caption_context_does_not_replace_the_stored_caption() -> Result<(), Error> {
    let parsed = parse(
        "= Doc\n:listing-caption: Listing\n\n.Title\n----\nCode.\n----\n",
        &Options::default(),
    )?;
    let block = parsed
        .document_storage()
        .blocks
        .first()
        .ok_or("listing")?
        .clone();
    let Block::DelimitedListing(crate::model::Listing(delimited)) = block else {
        return Err("delimited listing".into());
    };
    let stored = delimited.metadata.caption.clone().ok_or("stored caption")?;
    let block = Block::DelimitedExample(crate::model::Example(Delimited {
        metadata: delimited.metadata,
        body: Vec::new(),
        delimiter: delimited.delimiter,
        title: delimited.title,
        location: delimited.location,
        open_delimiter_location: delimited.open_delimiter_location,
        close_delimiter_location: delimited.close_delimiter_location,
        source_text: delimited.source_text,
    }));
    let block = BlockRef::from(&block);
    let kind = CaptionKind::for_block(block);
    assert_eq!(kind, Some(Example));
    assert_eq!(
        CaptionRef::resolve(
            block.attributes().ok_or("block attributes")?,
            parsed.document().attributes(),
            kind,
        ),
        Some(CaptionRef::from(&stored))
    );
    Ok(())
}
