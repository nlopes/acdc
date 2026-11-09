//! Scalar reads and reference identity preserve literal quote characters.

use acdc_parser::{
    CompatibilityMode, Options,
    ast::{BlockKind, BlockRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn hyperlink_ids_and_reference_catalog_keep_the_same_literal_value(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "link:https://example.org[Link,id=\"'link-π'\"] https://example.org[URL,id=\"'url-π'\"] mailto:a@example.org[Mail,id=\"'mail-π'\"]";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::Paragraph(paragraph) = parsed
        .document()
        .blocks()
        .next()
        .map(BlockRef::kind)
        .ok_or("paragraph")?
    else {
        return Err("paragraph".into());
    };
    let mut count = 0;
    for node in paragraph.content().inlines() {
        if matches!(
            node.kind(),
            InlineKind::Link(_) | InlineKind::UrlMacro(_) | InlineKind::Mailto(_)
        ) {
            let id = node.id().ok_or("ID")?.as_str();
            let reference = parsed
                .document()
                .references()
                .get(id)
                .ok_or_else(|| format!("missing exact ID {id:?}"))?;
            assert_eq!(
                reference.location().absolute_start(),
                node.location().absolute_start()
            );
            assert!(
                !parsed
                    .document()
                    .references()
                    .contains(id.trim_matches('\''))
            );
            count += 1;
        }
    }
    assert_eq!(count, 3);
    Ok(())
}
