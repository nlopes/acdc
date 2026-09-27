use acdc_parser::{Block, Options, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn standalone_anchor_interrupts_description_principal_text() -> Result<(), Error> {
    for gap in ["\n", "\n\n"] {
        let source = format!("Term:: Description.{gap}[[target,Target label]]\nFollowing.");
        let parsed = parse(&source, &Options::default())?;
        let [Block::DescriptionList(_), Block::Paragraph(paragraph)] =
            parsed.document().blocks.as_slice()
        else {
            return Err(format!("expected separate list and paragraph for {source:?}").into());
        };
        assert_eq!(paragraph.metadata.anchors.len(), 1);
        assert_eq!(
            paragraph
                .metadata
                .anchors
                .first()
                .ok_or("missing anchor")?
                .id,
            "target"
        );
    }
    Ok(())
}

#[test]
fn standalone_anchor_interrupts_an_ordinary_paragraph() -> Result<(), Error> {
    let source = "Before.\n[[target]]\nAfter.";
    let parsed = parse(source, &Options::default())?;
    let [Block::Paragraph(before), Block::Paragraph(after)] = parsed.document().blocks.as_slice()
    else {
        return Err("expected two paragraphs".into());
    };
    assert!(before.metadata.anchors.is_empty());
    assert_eq!(
        after.metadata.anchors.first().ok_or("missing anchor")?.id,
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
                let [Block::DescriptionList(list)] = parsed.document().blocks.as_slice() else {
                    return Err(format!("expected one description list for {source:?}").into());
                };
                let [item] = list.items.as_slice() else {
                    return Err("expected one description item".into());
                };
                let [nested] = item.description.as_slice() else {
                    return Err("expected one nested list".into());
                };
                let metadata = if let Block::UnorderedList(list) = nested {
                    &list.metadata
                } else if let Block::OrderedList(list) = nested {
                    &list.metadata
                } else {
                    return Err("expected an ordered or unordered list".into());
                };
                assert_eq!(metadata.anchors.len(), 1);
                assert_eq!(
                    metadata.anchors.first().ok_or("missing anchor")?.id,
                    "target"
                );
            }
            let source = format!("First:: Description.{gap}{metadata}\nNext:: Description.");
            let parsed = parse(&source, &Options::default())?;
            let [Block::DescriptionList(first), Block::DescriptionList(next)] =
                parsed.document().blocks.as_slice()
            else {
                return Err(format!("expected separate description lists for {source:?}").into());
            };
            assert!(first.metadata.anchors.is_empty());
            assert_eq!(
                next.metadata.anchors.first().ok_or("missing anchor")?.id,
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
        assert_eq!(parsed.document().blocks.len(), 1, "{source}");
    }
    Ok(())
}
