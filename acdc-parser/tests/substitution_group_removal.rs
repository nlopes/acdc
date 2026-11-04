use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn removed_groups_do_not_register_disabled_macros() -> Result<(), Error> {
    for subs in [
        "-normal",
        "-normal,attributes+",
        "-normal,+quotes",
        "+quotes,-normal",
        "-normal,+normal,-normal",
        "-normal,-verbatim,+attributes",
    ] {
        for newline in ["\n", "\r\n"] {
            let source = format!(
                ":value: anchor:from-value[]footnote:[Attribute note.]{newline}{newline}\
                 [subs=\"{subs}\"]{newline}\
                 {{value}} anchor:direct[]footnote:[Direct note.]{newline}"
            );
            let parsed = parse(&source, &Options::default())?;
            let expected = 0;
            assert_eq!(
                parsed.document_ref().footnotes().count(),
                expected,
                "{source}"
            );
            assert_eq!(
                parsed.document_ref().references().len(),
                expected,
                "{source}"
            );
        }
    }
    Ok(())
}

#[test]
fn readded_group_members_preserve_macro_registration_order() -> Result<(), Error> {
    for (subs, expected) in [
        ("-normal,+macros", 1),
        ("-normal,+attributes,+macros", 2),
        ("-normal,+macros,+attributes", 1),
        ("-normal,+normal", 2),
        ("-normal,+normal,-macros", 0),
        ("-verbatim,+verbatim", 2),
        ("-normal,+normal,-normal,+macros", 1),
    ] {
        let source = format!(
            ":value: anchor:from-value[]footnote:[Attribute note.]\n\n\
             [subs=\"{subs}\"]\n\
             {{value}} anchor:direct[]footnote:[Direct note.]\n"
        );
        let parsed = parse(&source, &Options::default())?;
        assert_eq!(
            parsed.document_ref().footnotes().count(),
            expected,
            "{subs}"
        );
        assert_eq!(parsed.document_ref().references().len(), expected, "{subs}");
        assert_eq!(
            parsed.document_ref().references().contains("direct"),
            expected > 0,
            "{subs}"
        );
        assert_eq!(
            parsed.document_ref().references().contains("from-value"),
            expected == 2,
            "{subs}"
        );
    }
    Ok(())
}

#[test]
fn removed_verbatim_group_controls_callouts_in_every_code_block_kind() -> Result<(), Error> {
    for (subs, enabled) in [
        ("-verbatim", false),
        ("-verbatim,+callouts", true),
        ("-verbatim,+verbatim", true),
        ("-verbatim,+verbatim,-verbatim", false),
        ("-verbatim,+specialchars", false),
        ("-normal", true),
        ("-normal,-verbatim", false),
    ] {
        for newline in ["\n", "\r\n"] {
            for (style, delimiter, indent) in [
                ("source,text,", "----", ""),
                ("", "....", ""),
                ("source,text,", "", ""),
                ("listing,", "", ""),
                ("literal,", "", ""),
                ("", "", " "),
            ] {
                let content = format!("{indent}café <1>");
                let body = if delimiter.is_empty() {
                    content
                } else {
                    format!("{delimiter}{newline}{content}{newline}{delimiter}")
                };
                let source = format!("[{style}subs=\"{subs}\"]{newline}{body}{newline}");
                let parsed = parse(&source, &Options::default())?;
                let nodes = match parsed.document_ref().blocks().next().map(BlockRef::kind) {
                    Some(BlockKind::Paragraph(paragraph)) => paragraph.content(),
                    Some(BlockKind::Listing(block)) => block.content(),
                    Some(BlockKind::Literal(block)) => block.content(),
                    _ => return Err("expected code content".into()),
                };
                let callouts = nodes
                    .inlines()
                    .filter_map(|node| {
                        if let InlineKind::Callout(callout) = node.kind() {
                            Some(callout)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                let expected = enabled;
                assert_eq!(callouts.len(), usize::from(expected), "{source}");
                for callout in &callouts {
                    assert_eq!(
                        parsed.source().get(
                            callout.location().absolute_start()..=callout.location().absolute_end()
                        ),
                        Some("<1>"),
                        "{source}"
                    );
                }
                if !expected {
                    let text = nodes
                        .inlines()
                        .filter_map(|node| {
                            if let InlineKind::Plain(text) = node.kind() {
                                Some(text.text())
                            } else if let InlineKind::Verbatim(text) = node.kind() {
                                Some(text.text())
                            } else {
                                None
                            }
                        })
                        .collect::<String>();
                    assert!(text.contains("café <1>"), "{source}: {text:?}");
                }
            }
        }
    }
    Ok(())
}
