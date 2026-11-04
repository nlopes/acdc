use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn standalone_anchor_interrupts_description_principal_text() -> Result<(), Error> {
    for gap in ["\n", "\n\n"] {
        let source = format!("Term:: Description.{gap}[[target,Target label]]\nFollowing.");
        let parsed = parse(&source, &Options::default())?;
        let [
            BlockKind::DescriptionList(_),
            BlockKind::Paragraph(paragraph),
        ] = &*parsed
            .document_ref()
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err(format!("expected separate list and paragraph for {source:?}").into());
        };
        assert_eq!(paragraph.anchors().count(), 1);
        assert_eq!(
            paragraph.anchors().next().ok_or("missing anchor")?.id(),
            "target"
        );
    }
    Ok(())
}

#[test]
fn standalone_anchor_interrupts_an_ordinary_paragraph() -> Result<(), Error> {
    let source = "Before.\n[[target]]\nAfter.";
    let parsed = parse(source, &Options::default())?;
    let [BlockKind::Paragraph(before), BlockKind::Paragraph(after)] = &*parsed
        .document_ref()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected two paragraphs".into());
    };
    assert!(before.anchors().next().is_none());
    assert_eq!(
        after.anchors().next().ok_or("missing anchor")?.id(),
        "target"
    );
    Ok(())
}

#[test]
fn description_list_metadata_belongs_to_the_following_list() -> Result<(), Error> {
    for gap in ["\n", "\n\n"] {
        for metadata in ["[[target]]", "[#target]", "[[target]]\n[.marked]"] {
            for marker in ["*", "."] {
                let source = format!("Term:: Description.{gap}{metadata}\n{marker} Nested.");
                let parsed = parse(&source, &Options::default())?;
                let [BlockKind::DescriptionList(list)] = &*parsed
                    .document_ref()
                    .blocks()
                    .map(BlockRef::kind)
                    .collect::<Vec<_>>()
                else {
                    return Err(format!("expected one description list for {source:?}").into());
                };
                let [item] = &*list.items().collect::<Vec<_>>() else {
                    return Err("expected one description item".into());
                };
                let [nested] = &*item.blocks().map(BlockRef::kind).collect::<Vec<_>>() else {
                    return Err("expected one nested list".into());
                };
                let anchors = if let BlockKind::UnorderedList(list) = nested {
                    list.anchors().collect::<Vec<_>>()
                } else if let BlockKind::OrderedList(list) = nested {
                    list.anchors().collect::<Vec<_>>()
                } else {
                    return Err("expected an ordered or unordered list".into());
                };
                assert_eq!(anchors.len(), 1);
                assert_eq!(anchors.first().ok_or("missing anchor")?.id(), "target");
            }
            let source = format!("First:: Description.{gap}{metadata}\nNext:: Description.");
            let parsed = parse(&source, &Options::default())?;
            let [
                BlockKind::DescriptionList(first),
                BlockKind::DescriptionList(next),
            ] = &*parsed
                .document_ref()
                .blocks()
                .map(BlockRef::kind)
                .collect::<Vec<_>>()
            else {
                return Err(format!("expected separate description lists for {source:?}").into());
            };
            assert!(first.anchors().next().is_none());
            assert_eq!(
                next.anchors().next().ok_or("missing anchor")?.id(),
                "target"
            );
        }
    }
    Ok(())
}

#[test]
fn inline_and_verbatim_anchor_lines_do_not_split_blocks() -> Result<(), Error> {
    for source in [
        "Before.\n[[target]]Inline text.",
        "Before.\n[[one]][[two]]\nAfter.",
        "Before.\n\\[[escaped]]\nAfter.",
        "* Before.\n[[target]]\nAfter.",
        ". Before.\n[[target]]\nAfter.",
        "[source]\nBefore.\n[[target]]\nAfter.",
        "[listing]\nBefore.\n[[target]]\nAfter.",
        "[literal]\nBefore.\n[[target]]\nAfter.",
    ] {
        let parsed = parse(source, &Options::default())?;
        assert_eq!(parsed.document_ref().blocks().count(), 1, "{source}");
    }
    Ok(())
}
