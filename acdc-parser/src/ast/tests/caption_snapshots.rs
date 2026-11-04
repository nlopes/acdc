use crate::{CaptionKind, Options, ast::CaptionRef, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn stored_captions_keep_their_snapshot_ordinal_and_content(
    #[values(crate::CompatibilityMode::Asciidoctor, crate::CompatibilityMode::Draft)]
    mode: crate::CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Doc\n\n.First\nimage::first.svg[]\n\n:figure-caption: Plate\n\n[caption='*Rich*: ']\n.Second\nimage::second.svg[]\n\n[caption='']\n.Empty\nimage::empty.svg[]\n\n:figure-caption!:\n\n.Disabled\nimage::disabled.svg[]\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    for block in &parsed.document_storage().blocks {
        if let Some(metadata) = block.metadata() {
            for fallback in [None, Some(CaptionKind::Table)] {
                let caption = CaptionRef::resolve(
                    crate::ast::BlockRef::from(block)
                        .attributes()
                        .ok_or("attributes")?,
                    parsed.document().attributes(),
                    fallback,
                );
                assert_eq!(caption, metadata.caption.as_ref().map(CaptionRef::from));
            }
        }
    }
    Ok(())
}
