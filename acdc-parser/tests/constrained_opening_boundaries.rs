use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn constrained_opening_punctuation_keeps_formatting_and_source_spans() -> Result<(), Error> {
    for prefix in ["=", "%", "$", "@", "<", ">", "&", "!", "«"] {
        for (marker, variant) in [("*", "bold"), ("_", "italic"), ("`", "code"), ("#", "mark")] {
            if prefix == "&" && marker == "#" {
                continue;
            }
            let quoted = format!("{marker}café{marker}");
            let source = format!("P {prefix}{quoted} Q.\n");
            let parsed = parse(&source, &Options::default())?;
            let [BlockKind::Paragraph(paragraph)] = &*parsed
                .document_ref()
                .blocks()
                .map(BlockRef::kind)
                .collect::<Vec<_>>()
            else {
                return Err("expected one paragraph".into());
            };
            let span = paragraph
                .content()
                .inlines()
                .find(|node| {
                    matches!(
                        (variant, node.kind()),
                        ("bold", InlineKind::Bold(_))
                            | ("italic", InlineKind::Italic(_))
                            | ("code", InlineKind::Monospace(_))
                            | ("mark", InlineKind::Highlight(_))
                    )
                })
                .ok_or_else(|| format!("missing {variant}: {source:?}"))?
                .location();
            assert_eq!(
                &parsed.source()[span.absolute_start()..=span.absolute_end()],
                quoted,
                "{source:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn constrained_opening_barriers_stay_literal() -> Result<(), Error> {
    for prefix in [":", ";", "}", "A", "1", "_", "café", "日", "&amp;", "&#42;"] {
        for marker in ["*", "_", "`", "#"] {
            if prefix == "_" && marker == "_" {
                continue;
            }
            let source = format!("P {prefix}{marker}word{marker} Q.\n");
            let parsed = parse(&source, &Options::default())?;
            let [BlockKind::Paragraph(paragraph)] = &*parsed
                .document_ref()
                .blocks()
                .map(BlockRef::kind)
                .collect::<Vec<_>>()
            else {
                return Err("expected one paragraph".into());
            };
            assert!(
                paragraph.content().inlines().all(|node| !matches!(
                    node.kind(),
                    InlineKind::Bold(_)
                        | InlineKind::Italic(_)
                        | InlineKind::Monospace(_)
                        | InlineKind::Highlight(_)
                )),
                "unexpected formatting: {source:?}"
            );
        }
    }
    let parsed = parse("P &#word# Q.\n", &Options::default())?;
    let [BlockKind::Paragraph(paragraph)] = &*parsed
        .document_ref()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected one paragraph".into());
    };
    assert!(
        paragraph
            .content()
            .inlines()
            .all(|node| !matches!(node.kind(), InlineKind::Highlight(_)))
    );
    Ok(())
}

#[test]
fn constrained_opening_after_raw_tags_keeps_nested_source_spans() -> Result<(), Error> {
    for newline in ["\n", "\r\n"] {
        let source = format!("P pass:q[<del>*bold _café_*</del>] Q.{newline}");
        let parsed = parse(&source, &Options::default())?;
        let [BlockKind::Paragraph(paragraph)] = &*parsed
            .document_ref()
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("expected one paragraph".into());
        };
        let bold = paragraph
            .content()
            .inlines()
            .find_map(|node| {
                if let InlineKind::Bold(bold) = node.kind() {
                    Some(bold)
                } else {
                    None
                }
            })
            .ok_or("missing bold after raw tag")?;
        assert_eq!(
            &parsed.source()[bold.location().absolute_start()..=bold.location().absolute_end()],
            "*bold _café_*"
        );
        let italic = bold
            .content()
            .inlines()
            .find(|node| matches!(node.kind(), InlineKind::Italic(_)))
            .ok_or("missing nested italic")?
            .location();
        assert_eq!(
            &parsed.source()[italic.absolute_start()..=italic.absolute_end()],
            "_café_"
        );
    }
    Ok(())
}
