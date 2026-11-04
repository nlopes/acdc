use crate::{
    Block, InlineMacro, InlineNode, Options,
    ast::{
        BlockKind, BlockRef, ContentRef, DocumentRef, InlineKind, InlineRef, LocationRef,
        ParagraphRef, PositionRef, SectionRef,
    },
    parse,
};

type Error = Box<dyn std::error::Error>;

fn plain(content: ContentRef<'_>) -> Result<&str, Error> {
    let mut inlines = content.inlines();
    let Some(inline) = inlines.next() else {
        return Err("missing inline".into());
    };
    let InlineKind::Plain(text) = inline.kind() else {
        return Err("expected plain text".into());
    };
    assert!(inlines.next().is_none());
    Ok(text.text())
}

#[test]
fn owner_conversions_preserve_typed_content_and_locations() -> Result<(), Error> {
    let parsed = parse(
        "== Section\n\nimage:bird.png[Bird]\n\n|===\n|Cell\n|===\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    assert_eq!(DocumentRef::from(document).blocks().count(), 1);
    let block = document.blocks.first().ok_or("missing section")?;
    assert!(matches!(
        BlockRef::from(block).kind(),
        BlockKind::Section(_)
    ));
    let Block::Section(section) = block else {
        return Err("expected section".into());
    };
    assert_eq!(SectionRef::from(section).level(), 1);
    let Some(Block::Paragraph(paragraph)) = section.content.first() else {
        return Err("missing paragraph".into());
    };
    assert_eq!(ParagraphRef::from(paragraph).content().inlines().count(), 1);
    let inline = paragraph.content.first().ok_or("missing inline image")?;
    assert!(matches!(
        InlineRef::from(inline).kind(),
        InlineKind::Image(_)
    ));
    let InlineNode::Macro(inline_macro) = inline else {
        return Err("expected macro".into());
    };
    assert!(matches!(
        InlineRef::from(inline_macro).kind(),
        InlineKind::Image(_)
    ));
    let InlineMacro::Image(image) = inline_macro else {
        return Err("expected image".into());
    };
    assert_eq!(
        plain(ContentRef::from(image.alt.as_ref().ok_or("missing alt")?))?,
        "Bird"
    );
    let Some(Block::DelimitedBlock(table)) = section.content.get(1) else {
        return Err("missing table".into());
    };
    assert!(matches!(BlockRef::from(table).kind(), BlockKind::Table(_)));
    assert_eq!(LocationRef::from(&paragraph.location).start().line(), 3);
    assert_eq!(PositionRef::from(&paragraph.location.start).column(), 1);
    Ok(())
}
