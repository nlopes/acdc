use acdc_converters_core::code::{detect_language, source_language};
use acdc_parser::{
    AttributeValue, BlockMetadata, CompatibilityMode, Location, Options, Paragraph,
    ast::{BlockKind, ParagraphRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn source_languages_keep_exact_text_and_require_the_source_style() -> Result<(), Error> {
    let values = [
        (None, None),
        (Some(AttributeValue::Bool(false)), None),
        (Some(AttributeValue::Bool(true)), None),
        (Some(AttributeValue::None), None),
        (Some(AttributeValue::from("")), Some("")),
        (Some(AttributeValue::from("text")), Some("text")),
        (
            Some(AttributeValue::from("custom-language")),
            Some("custom-language"),
        ),
        (Some(AttributeValue::from("'rust'")), Some("'rust'")),
        (Some(AttributeValue::from("\"rust\"")), Some("\"rust\"")),
        (
            Some(AttributeValue::from("  α-language  ")),
            Some("  α-language  "),
        ),
    ];
    for style in [
        None,
        Some(""),
        Some("source"),
        Some("Source"),
        Some("listing"),
        Some("literal"),
    ] {
        for (value, language) in &values {
            let mut paragraph = Paragraph::new(Vec::new(), Location::default());
            let mut metadata = BlockMetadata::default();
            metadata.style = style;
            metadata.attributes.set("style".into(), "source".into());
            if let Some(value) = value {
                metadata.attributes.set("language".into(), value.clone());
            }
            assert!(paragraph.set_metadata(metadata.clone()).is_err());
            let mut prepared = metadata.clone();
            prepared.attributes.remove("style");
            paragraph.set_metadata(prepared)?;
            let expected = if style == Some("source") {
                *language
            } else {
                None
            };
            assert_eq!(
                source_language(ParagraphRef::from(&paragraph).attributes()),
                expected,
                "{style:?} {value:?}"
            );
            assert_eq!(detect_language(&metadata), expected, "{style:?} {value:?}");
        }
    }
    Ok(())
}

#[test]
fn language_text_borrows_the_current_owner_value() -> Result<(), Error> {
    let mut paragraph = Paragraph::new(Vec::new(), Location::default());
    let mut metadata = BlockMetadata::default();
    metadata.style = Some("source");
    let language = String::from("owned language");
    let pointer = language.as_ptr();
    metadata.attributes.set("language".into(), language.into());
    paragraph.set_metadata(metadata)?;
    let text = {
        let attributes = ParagraphRef::from(&paragraph).attributes();
        source_language(attributes).ok_or("language")?
    };
    assert_eq!(text.as_ptr(), pointer);
    assert_eq!(text, "owned language");
    let mut metadata = paragraph.take_metadata();
    metadata.attributes.set("language".into(), "edited".into());
    paragraph.set_metadata(metadata)?;
    assert_eq!(
        source_language(ParagraphRef::from(&paragraph).attributes()),
        Some("edited")
    );
    assert_eq!(detect_language(&paragraph.clone_metadata()), Some("edited"));
    let mut metadata = paragraph.take_metadata();
    metadata.style = Some("listing");
    paragraph.set_metadata(metadata)?;
    assert_eq!(
        source_language(ParagraphRef::from(&paragraph).attributes()),
        None
    );
    assert_eq!(detect_language(&paragraph.clone_metadata()), None);
    Ok(())
}

#[test]
fn parsed_paragraphs_and_delimited_blocks_keep_language_controls() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for (source, expected) in [
            ("[source,rust]\ntext", Some("rust")),
            ("[source,custom-language]\ntext", Some("custom-language")),
            ("[source,language=\"\"]\ntext", Some("")),
            ("[source]\ntext", None),
            ("[listing,language=python]\ntext", None),
            ("[source,rust]\n----\ntext\n----", Some("rust")),
            (
                "[source,custom-language]\n----\ntext\n----",
                Some("custom-language"),
            ),
        ] {
            let options = Options::builder().with_compatibility_mode(mode).build()?;
            let parsed = parse(source, &options)?;
            let parsed_blocks: Vec<_> = parsed
                .document()
                .blocks()
                .map(acdc_parser::Block::from)
                .collect();
            let block = parsed_blocks.first().ok_or("block")?;
            let metadata = if let acdc_parser::Block::Paragraph(paragraph) = block {
                paragraph.clone_metadata()
            } else if let acdc_parser::Block::DelimitedBlock(block) = block {
                block.clone_metadata()
            } else {
                return Err("expected source or listing block".into());
            };
            assert_eq!(detect_language(&metadata), expected, "{mode:?} {source}");
            let kind = parsed
                .document()
                .blocks()
                .next()
                .ok_or("typed block")?
                .kind();
            let attributes = if let BlockKind::Paragraph(paragraph) = kind {
                paragraph.attributes()
            } else if let BlockKind::Listing(listing) = kind {
                listing.attributes()
            } else {
                return Err("expected typed paragraph or listing".into());
            };
            assert_eq!(source_language(attributes), expected, "{mode:?} {source}");
        }
    }
    Ok(())
}
