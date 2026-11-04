use crate::{
    Block, CompatibilityMode, ContentAttribute, ElementId, Image, InlineImage, InlineMacro,
    InlineNode, Location, Options, Source,
    ast::{
        BlockKind, BlockRef, BoldRef, ContentRef, ImageRef, InlineImageRef, InlineKind, InlineRef,
        TableRef,
    },
    parse,
};

type Error = Box<dyn std::error::Error>;

fn plain(content: ContentRef<'_>) -> Result<&str, Error> {
    let mut inlines = content.inlines();
    let Some(InlineKind::Plain(text)) = inlines.next().map(InlineRef::kind) else {
        return Err("expected plain text".into());
    };
    assert!(inlines.next().is_none());
    Ok(text.text())
}

fn bold_word(bold: BoldRef<'_>) -> Result<&str, Error> {
    plain(bold.content())
}

fn table(block: BlockRef<'_>) -> Result<TableRef<'_>, Error> {
    let BlockKind::Table(table) = block.kind() else {
        return Err("expected table".into());
    };
    Ok(table)
}

fn image_in(block: BlockRef<'_>) -> Result<InlineImageRef<'_>, Error> {
    let BlockKind::Paragraph(paragraph) = block.kind() else {
        return Err("expected paragraph".into());
    };
    let mut inlines = paragraph.content().inlines();
    let Some(InlineKind::Image(image)) = inlines.next().map(InlineRef::kind) else {
        return Err("expected inline image".into());
    };
    assert!(inlines.next().is_none());
    Ok(image)
}

#[rstest::rstest]
fn images_preserve_independent_content_and_identity(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        ".Caption\n[#block,alt='*Bird*']\nimage::bird.png[]\n\n\
         image:inline.png[alt=\"\",title=\"\",id=inline]\n\n\
         .Table title\n[#table]\n|===\n|Cell\n|===\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut blocks = parsed.document().blocks();
    let Some(BlockKind::Image(image)) = blocks.next().map(BlockRef::kind) else {
        return Err("missing block image".into());
    };
    assert_eq!(plain(image.title().ok_or("missing caption")?)?, "Caption");
    let alt = image
        .alt()
        .ok_or("missing alt")?
        .inlines()
        .next()
        .map(InlineRef::kind)
        .ok_or("empty alt")?;
    if mode == CompatibilityMode::Draft {
        let InlineKind::Raw(alt) = alt else {
            return Err("expected literal alt".into());
        };
        assert_eq!(alt.text(), "*Bird*");
    } else {
        let InlineKind::Bold(alt) = alt else {
            return Err("expected bold alt".into());
        };
        assert_eq!(bold_word(alt)?, "Bird");
    }
    assert_eq!(image.id().ok_or("missing block ID")?.as_str(), "block");
    assert_eq!(
        image
            .id()
            .ok_or("missing block ID")?
            .location()
            .start()
            .line(),
        2
    );
    assert_eq!(image.source().to_string(), "bird.png");
    let inline = image_in(blocks.next().ok_or("missing inline image")?)?;
    assert!(inline.alt().is_some_and(ContentRef::is_empty));
    assert!(inline.tooltip().is_some_and(ContentRef::is_empty));
    assert_eq!(inline.id().ok_or("missing inline ID")?.as_str(), "inline");
    let Some(Block::Paragraph(paragraph)) = parsed.document_storage().blocks.get(1) else {
        return Err("missing owner paragraph".into());
    };
    let Some(InlineNode::Macro(InlineMacro::Image(owner))) = paragraph.content.first() else {
        return Err("missing owner image".into());
    };
    assert_eq!(
        parsed.resolve_location(inline.id().ok_or("missing inline ID")?.location()),
        parsed.source_location(
            &owner
                .metadata
                .id
                .as_ref()
                .ok_or("missing owner ID")?
                .location
        )
    );
    let table = table(blocks.next().ok_or("missing titled table")?)?;
    assert_eq!(
        plain(table.title().ok_or("missing table title")?)?,
        "Table title"
    );
    assert_eq!(table.id().ok_or("missing table ID")?.as_str(), "table");
    assert_eq!(
        table
            .id()
            .ok_or("missing table ID")?
            .location()
            .start()
            .line(),
        8
    );

    let mut image = Image::new(Source::Name("bird"), Location::default());
    assert!(ImageRef::from(&image).alt().is_none());
    assert!(ImageRef::from(&image).title().is_none());
    image.alt = Some(ContentAttribute::default());
    assert!(
        ImageRef::from(&image)
            .alt()
            .is_some_and(ContentRef::is_empty)
    );
    let mut inline = InlineImage::new(Source::Name("bird"), Location::default());
    assert!(InlineImageRef::from(&inline).alt().is_none());
    assert!(InlineImageRef::from(&inline).tooltip().is_none());
    inline.tooltip = Some(ContentAttribute::default());
    inline.metadata.id = Some(ElementId::new("constructed", Location::default()));
    assert!(
        InlineImageRef::from(&inline)
            .tooltip()
            .is_some_and(ContentRef::is_empty)
    );
    assert_eq!(
        InlineImageRef::from(&inline)
            .id()
            .ok_or("missing constructed ID")?
            .as_str(),
        "constructed"
    );
    Ok(())
}
