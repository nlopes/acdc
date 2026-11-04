use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

// Byte offsets currently use line-ending-normalized source. Check that each
// inclusive span still covers references removed by inline substitution.
#[test]
fn empty_quoted_attributes_preserve_original_source_spans() -> Result<(), Error> {
    for marker in ["*", "_", "`", "#", "^", "~", "**", "__", "``", "##"] {
        for references in ["{blank}", "{blank}{blank}"] {
            for newline in ["\n", "\r\n"] {
                let quoted = format!("{marker}{references}{marker}");
                let source =
                    format!("= Empty{newline}:blank:{newline}{newline}α {quoted} ω.{newline}");
                let parsed = parse(&source, &Options::default())?;
                let [BlockKind::Paragraph(paragraph)] = &*parsed
                    .document_ref()
                    .blocks()
                    .map(BlockRef::kind)
                    .collect::<Vec<_>>()
                else {
                    return Err("expected a paragraph".into());
                };
                let nodes = paragraph.content().inlines().collect::<Vec<_>>();
                let [prefix, formatted, suffix] = nodes.as_slice() else {
                    return Err(format!("expected one formatted span: {nodes:?}").into());
                };
                assert!(matches!(prefix.kind(), InlineKind::Plain(_)));
                assert!(matches!(suffix.kind(), InlineKind::Plain(_)));
                assert!(
                    !matches!(formatted.kind(), InlineKind::Plain(_)),
                    "{source:?}"
                );
                let location = formatted.location();
                assert_eq!(
                    &parsed.source()[location.absolute_start()..=location.absolute_end()],
                    quoted,
                    "{source:?}"
                );
                assert_eq!(location.start().line(), 4);
                assert_eq!(location.end().line(), 4);
                assert_eq!(location.start().column(), 3);
                assert_eq!(location.end().column() as usize, 2 + quoted.len());
            }
        }
    }
    Ok(())
}
