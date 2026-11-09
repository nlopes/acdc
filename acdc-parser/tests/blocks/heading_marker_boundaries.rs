use acdc_parser::{
    Options,
    ast::{AnchorRef, BlockKind, BlockRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn bare_heading_markers_keep_one_paragraph_and_complete_source_spans() -> Result<(), Error> {
    for marker in [
        "#", "##", "###", "####", "#####", "######", "#######", "=", "==", "===",
    ] {
        for padding in ["", " ", "\t", " \t"] {
            for newline in ["\n", "\r\n"] {
                for ending in ["", newline] {
                    let source =
                        format!("Before.{newline}{marker}{padding}{newline}After.{ending}");
                    let parsed = parse(&source, &Options::default())?;
                    let [BlockKind::Paragraph(paragraph)] = &*parsed
                        .document()
                        .blocks()
                        .map(BlockRef::kind)
                        .collect::<Vec<_>>()
                    else {
                        return Err(format!("expected one paragraph: {source:?}").into());
                    };
                    let span = paragraph.location();
                    assert_eq!(span.start().line(), 1, "{source:?}");
                    assert_eq!(span.end().line(), 3, "{source:?}");
                    assert_eq!(span.end().column(), 6, "{source:?}");
                    assert_eq!(
                        &parsed.preprocessed_source()[span.absolute_start()..=span.absolute_end()],
                        format!("Before.\n{marker}\nAfter."),
                        "{source:?}"
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn bare_heading_markers_at_document_edges_are_not_titles() -> Result<(), Error> {
    for marker in ["=", "==", "===", "#", "##", "###", "######", "#######"] {
        for padding in ["", " ", "\t", " \t"] {
            for ending in ["", "\n", "\r\n"] {
                for before in ["", "Before.\n"] {
                    let source = format!("{before}{marker}{padding}{ending}");
                    let parsed = parse(&source, &Options::default())?;
                    assert!(parsed.document().header().is_none(), "{source:?}");
                    assert!(
                        matches!(
                            &*parsed
                                .document()
                                .blocks()
                                .map(BlockRef::kind)
                                .collect::<Vec<_>>(),
                            [BlockKind::Paragraph(_)]
                        ),
                        "{source:?}"
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn standalone_hashes_close_multiline_highlights() -> Result<(), Error> {
    for newline in ["\n", "\r\n"] {
        let source = format!("Before ##café{newline}##{newline}after.{newline}");
        let parsed = parse(&source, &Options::default())?;
        let [BlockKind::Paragraph(paragraph)] = &*parsed
            .document()
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("expected one paragraph".into());
        };
        let highlight = paragraph
            .content()
            .inlines()
            .find_map(|node| {
                if let InlineKind::Highlight(text) = node.kind() {
                    Some(text)
                } else {
                    None
                }
            })
            .ok_or("missing highlighted span")?;
        let span = highlight.location();
        assert_eq!(
            &parsed.preprocessed_source()[span.absolute_start()..=span.absolute_end()],
            "##café\n##"
        );
        assert_eq!(span.start().line(), 1);
        assert_eq!(span.end().line(), 2);
        assert_eq!(span.end().column(), 2);
    }
    Ok(())
}

#[test]
fn titled_headings_still_interrupt_paragraphs() -> Result<(), Error> {
    for marker in ["=", "#"] {
        for depth in 2..=6 {
            for separator in [" ", "\t", "  ", " \t "] {
                let source = format!(
                    "Before.\n{}{separator}Section\n\nBody.\n",
                    marker.repeat(depth)
                );
                let parsed = parse(&source, &Options::default())?;
                let [BlockKind::Paragraph(_), BlockKind::Section(section)] = &*parsed
                    .document()
                    .blocks()
                    .map(BlockRef::kind)
                    .collect::<Vec<_>>()
                else {
                    return Err(format!("expected a real section: {source:?}").into());
                };
                assert_eq!(usize::from(section.level()), depth - 1, "{source:?}");
                assert!(
                    matches!(
                        &*section.blocks().map(BlockRef::kind).collect::<Vec<_>>(),
                        [BlockKind::Paragraph(_)]
                    ),
                    "{source:?}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn heading_metadata_before_bare_markers_belongs_to_the_paragraph() -> Result<(), Error> {
    for marker in ["##", "=="] {
        for metadata in ["[#literal]", "[discrete#literal]"] {
            let source = format!(
                "Before.\n\n{metadata}\n.Title footnote:[Note.]\n{marker}\nAfter.\n\n== Real\n\nBody.\n"
            );
            let parsed = parse(&source, &Options::default())?;
            let [
                BlockKind::Paragraph(_),
                BlockKind::Paragraph(paragraph),
                BlockKind::Section(_),
            ] = &*parsed
                .document()
                .blocks()
                .map(BlockRef::kind)
                .collect::<Vec<_>>()
            else {
                return Err(format!("expected paragraph metadata: {source:?}").into());
            };
            let mut anchors = paragraph.anchors();
            assert_eq!(
                anchors.next().map(AnchorRef::id),
                Some("literal"),
                "{source:?}"
            );
            assert!(anchors.next().is_none(), "{source:?}");
            assert_eq!(parsed.document().footnotes().count(), 1);
        }
    }
    Ok(())
}

#[test]
fn a_title_that_expands_to_empty_is_still_a_heading() -> Result<(), Error> {
    let source = "= Document\n:blank:\n\nBefore.\n== {blank}\n\nBody.\n";
    let parsed = parse(source, &Options::default())?;
    assert!(matches!(
        &*parsed
            .document()
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>(),
        [BlockKind::Paragraph(_), BlockKind::Section(_)]
    ));
    Ok(())
}
