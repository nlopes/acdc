use acdc_parser::{
    CompatibilityMode, Options, SafeMode,
    ast::{BlockKind, BlockRef, InlineKind, StructuralRef},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("* First\n+\nAttached.\n* Second footnote:[Once]\n")]
#[case("1. First\n+\nAttached.\n10. Second footnote:[Once]\n")]
#[case("Term:: First\n+\nAttached.\nOther:: Second footnote:[Once]\n")]
fn only_matching_sibling_or_ancestor_items_interrupt_asciidoctor_attachments(
    #[case] source: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let items = parsed
        .document()
        .descendants()
        .filter(|node| {
            matches!(
                node,
                StructuralRef::ListItem(_) | StructuralRef::DescriptionListItem(_)
            )
        })
        .count();
    assert_eq!(
        items,
        if mode == CompatibilityMode::Asciidoctor {
            2
        } else {
            1
        }
    );
    assert_eq!(parsed.document().footnotes().count(), 1);
    Ok(())
}

#[rstest::rstest]
#[case("** Nested")]
#[case(". Other family")]
#[case("- Other bullet")]
fn nested_or_other_family_markers_stay_in_an_attached_paragraph(
    #[case] line: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("* First\n+\nAttached.\n{line}\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(BlockKind::UnorderedList(list)) =
        parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("expected unordered list".into());
    };
    let item = list.items().next().ok_or("missing item")?;
    assert_eq!(list.items().count(), 1);
    let Some(BlockKind::Paragraph(paragraph)) = item.blocks().next().map(BlockRef::kind) else {
        return Err("expected attached paragraph".into());
    };
    assert_eq!(paragraph.location().end().line(), 4);
    assert_eq!(item.blocks().count(), 1);
    Ok(())
}

#[test]
fn list_markers_do_not_interrupt_ordinary_text_but_standalone_plus_does() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            "Paragraph\n* Wrapped\n. Wrapped too\n+\nNext paragraph.\n",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        assert_eq!(parsed.document().blocks().count(), 2);
        let Some(BlockKind::Paragraph(first)) =
            parsed.document().blocks().next().map(BlockRef::kind)
        else {
            return Err("expected ordinary paragraph".into());
        };
        assert_eq!(first.location().end().line(), 3);
        let Some(BlockKind::Paragraph(second)) =
            parsed.document().blocks().nth(1).map(BlockRef::kind)
        else {
            return Err("expected paragraph after plus".into());
        };
        assert_eq!(second.location().start().line(), 4);
    }
    Ok(())
}

#[test]
fn attached_selected_include_keeps_each_modes_original_text_range() -> Result<(), Error> {
    let root = tempfile::tempdir()?;
    let main = root.path().join("main.adoc");
    let child = root.path().join("child.adoc");
    let source = "Excluded.\n* First\n+\nAttached π.\n* Second footnote:[Once]\nExcluded tail.\n";
    std::fs::write(&child, source)?;
    std::fs::write(&main, "include::child.adoc[lines=2..5]\n")?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse_file(
            &main,
            &Options::builder()
                .with_compatibility_mode(mode)
                .with_safe_mode(SafeMode::Unsafe)
                .build()?,
        )?;
        let Some(BlockKind::UnorderedList(list)) =
            parsed.document().blocks().next().map(BlockRef::kind)
        else {
            return Err("expected list".into());
        };
        let Some(BlockKind::Paragraph(paragraph)) = list
            .items()
            .next()
            .ok_or("missing item")?
            .blocks()
            .next()
            .map(BlockRef::kind)
        else {
            return Err("expected attachment".into());
        };
        assert_eq!(
            paragraph.location().absolute_start(),
            source.find("Attached").ok_or("missing start")?
        );
        assert_eq!(paragraph.location().start().line(), 4);
        let end_line = if mode == CompatibilityMode::Asciidoctor {
            4
        } else {
            5
        };
        assert_eq!(paragraph.location().end().line(), end_line);
        let end = if mode == CompatibilityMode::Asciidoctor {
            source.find("π.").ok_or("missing end")? + "π".len()
        } else {
            source.find("]\n").ok_or("missing note end")?
        };
        assert_eq!(paragraph.location().absolute_end(), end);
        assert_eq!(parsed.document().footnotes().count(), 1);
        let text = parsed
            .document()
            .descendants()
            .find_map(|node| {
                let StructuralRef::Block(block) = node else {
                    return None;
                };
                let BlockKind::Paragraph(paragraph) = block.kind() else {
                    return None;
                };
                Some(
                    paragraph
                        .content()
                        .inlines()
                        .filter_map(|inline| {
                            if let InlineKind::Plain(text) = inline.kind() {
                                Some(text.text())
                            } else {
                                None
                            }
                        })
                        .collect::<String>(),
                )
            })
            .ok_or("missing text")?;
        assert!(text.starts_with("Attached π."));
    }
    Ok(())
}

#[test]
fn a_blank_line_after_plus_does_not_turn_a_sibling_into_paragraph_text() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            "* First\n+\n\n* Second\n",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let Some(BlockKind::UnorderedList(list)) =
            parsed.document().blocks().next().map(BlockRef::kind)
        else {
            return Err("expected list".into());
        };
        assert_eq!(list.items().count(), 2);
        assert_eq!(
            list.items().next().ok_or("missing item")?.blocks().count(),
            0
        );
    }
    Ok(())
}

#[rstest::rstest]
#[case(". First\n+\nAttached.\n10. Wrapped\n")]
#[case("1. First\n+\nAttached.\n. Wrapped\n")]
fn explicit_and_implicit_ordered_markers_do_not_interrupt_each_others_attachments(
    #[case] source: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(BlockKind::OrderedList(list)) = parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("expected ordered list".into());
    };
    assert_eq!(list.items().count(), 1);
    let Some(BlockKind::Paragraph(paragraph)) = list
        .items()
        .next()
        .ok_or("missing item")?
        .blocks()
        .next()
        .map(BlockRef::kind)
    else {
        return Err("expected attachment".into());
    };
    assert_eq!(paragraph.location().end().line(), 4);
    Ok(())
}

#[test]
fn an_ancestor_can_interrupt_an_asciidoctor_attachment() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            "* Parent\n** Nested\n+\nAttached.\n* Ancestor\n",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let items = parsed
            .document()
            .descendants()
            .filter(|node| matches!(node, StructuralRef::ListItem(_)))
            .count();
        assert_eq!(
            items,
            if mode == CompatibilityMode::Asciidoctor {
                3
            } else {
                2
            }
        );
    }
    Ok(())
}

#[rstest::rstest]
#[case("* First\n+\nAttached.\n[.child]\n** Nested footnote:[Once]\n")]
#[case(". First\n+\nAttached.\n[.child]\n.. Nested footnote:[Once]\n")]
fn metadata_after_an_attachment_keeps_the_nested_list_in_its_parent(
    #[case] source: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.document().blocks().count(), 1);
    let blocks = match parsed.document().blocks().next().map(BlockRef::kind) {
        Some(BlockKind::UnorderedList(list)) => list
            .items()
            .next()
            .ok_or("missing item")?
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>(),
        Some(BlockKind::OrderedList(list)) => list
            .items()
            .next()
            .ok_or("missing item")?
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>(),
        _ => return Err("expected list".into()),
    };
    assert_eq!(blocks.len(), 2);
    assert!(matches!(blocks.first(), Some(BlockKind::Paragraph(_))));
    assert!(matches!(
        blocks.get(1),
        Some(BlockKind::UnorderedList(_) | BlockKind::OrderedList(_))
    ));
    assert_eq!(parsed.document().footnotes().count(), 1);
    Ok(())
}
