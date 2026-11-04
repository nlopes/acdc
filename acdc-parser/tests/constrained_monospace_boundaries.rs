use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

// The JSON fixtures cannot assert that an inclusive location still slices the
// original source after an empty attribute expansion or UTF-8 text.
#[test]
fn constrained_monospace_boundaries_preserve_separate_source_spans() -> Result<(), Error> {
    for (prefix, suffix) in [
        ("", "**End**"),
        ("", "*End*"),
        ("", "#End#"),
        ("*Start*", ""),
        ("**Start**", ""),
        ("#Start#", ""),
    ] {
        for content in [
            "café",
            "{blank}",
            "anchor:zero[]",
            "https://example.org[Site]",
        ] {
            for newline in ["\n", "\r\n"] {
                let quoted = format!("`{content}`");
                let source =
                    format!(":blank:{newline}{newline}α {prefix}{quoted}{suffix} ω.{newline}");
                let parsed = parse(&source, &Options::default())?;
                let [BlockKind::Paragraph(paragraph)] = &*parsed
                    .document()
                    .blocks()
                    .map(BlockRef::kind)
                    .collect::<Vec<_>>()
                else {
                    return Err("expected one paragraph".into());
                };
                let mono = paragraph
                    .content()
                    .inlines()
                    .find(|node| matches!(node.kind(), InlineKind::Monospace(_)))
                    .ok_or_else(|| format!("missing monospace span: {source:?}"))?;
                let span = mono.location();
                assert_eq!(
                    &parsed.source()[span.absolute_start()..=span.absolute_end()],
                    quoted,
                    "{source:?}"
                );
                for sibling in [prefix, suffix].into_iter().filter(|text| !text.is_empty()) {
                    assert!(
                        paragraph.content().inlines().any(|node| {
                            matches!(node.kind(), InlineKind::Bold(_) | InlineKind::Highlight(_))
                                && {
                                    let span = node.location();
                                    &parsed.source()[span.absolute_start()..=span.absolute_end()]
                                        == sibling
                                }
                        }),
                        "missing separate sibling {sibling:?}: {source:?}"
                    );
                }
            }
        }
    }
    Ok(())
}
