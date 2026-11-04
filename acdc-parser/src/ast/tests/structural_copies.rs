use crate::{
    Block, Comment, CommentKind, CompatibilityMode, Document, Location, Options,
    ast::{BlockRef, DocumentRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn structural_copies_preserve_complete_backing_values(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    let mut kinds = std::collections::HashSet::new();
    for source in [
        "[#doc.role]\n= Doc: Subtitle\nÉlodie Marie Durand <elodie@example.org>\nv2.1, 2026-11-04: Reviewed\n:sectnums:\n\n[#section,reftext='*Chosen*']\n== Heading\n\nText footnote:[Note].\n\n.Figure\nimage::bird.svg[alt='*Bird*']\n",
        "toc::[]",
        "NOTE: Text.",
        "[discrete]\n== Heading",
        "Text.\n\n:name: value\n\nMore.",
        "'''",
        "<<<",
        "* Item\n+\nAttached.",
        ". Item",
        "<1> Item",
        "Term:: Value",
        "== Heading",
        "Paragraph.",
        "image::image.png[Alt]",
        "audio::audio.ogg[]",
        "video::video.mp4[]",
        "////\nComment\n////",
        "====\nExample\n====",
        "----\nCode <1>\n----",
        "....\nLiteral\n....",
        "--\nOpen\n--",
        "****\nSidebar\n****",
        "[cols=a]\n|===\n|* Nested\n|===",
        "++++\n<b>raw</b>\n++++",
        "____\nQuote\n____",
        "[verse]\n____\nVerse\n____",
        "[stem]\n++++\nx^2\n++++",
    ] {
        let parsed = parse(source, &options)?;
        let original = parsed.document();
        let copy = Document::from(parsed.document_ref());
        assert_eq!(&copy, original, "{source}");
        assert_eq!(format!("{copy:?}"), format!("{original:?}"), "{source}");
        for block in &original.blocks {
            kinds.insert(format!(
                "{:?}",
                std::mem::discriminant(&BlockRef::from(block).kind())
            ));
            let copy = Block::from(BlockRef::from(block));
            assert_eq!(&copy, block, "{source}");
            assert_eq!(format!("{copy:?}"), format!("{block:?}"), "{source}");
        }
    }
    let block = Block::Comment(Comment {
        content: "comment",
        kind: CommentKind::Line,
        location: Location::default(),
    });
    kinds.insert(format!(
        "{:?}",
        std::mem::discriminant(&BlockRef::from(&block).kind())
    ));
    assert_eq!(Block::from(BlockRef::from(&block)), block);
    assert_eq!(kinds.len(), 27, "{kinds:?}");
    let empty = Document::default();
    assert_eq!(Document::from(DocumentRef::from(&empty)), empty);
    Ok(())
}
