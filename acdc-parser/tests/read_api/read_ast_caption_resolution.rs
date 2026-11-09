use acdc_parser::{CaptionKind, Options, ast::CaptionRef, parse};

type Error = Box<dyn std::error::Error>;

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
