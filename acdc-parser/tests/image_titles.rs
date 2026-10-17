use acdc_parser::{Block, Caption, InlineMacro, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn block_image_title_attributes_follow_authored_precedence() -> Result<(), Error> {
    for (metadata, attributes, expected) in [
        ("", "title=Caption", "Caption"),
        (".Line\n", "title=Macro", "Macro"),
        ("[title=Metadata]\n", "", "Metadata"),
        ("[title=Metadata]\n.Line\n", "", "Line"),
        (".Line\n[title=Metadata]\n", "", "Metadata"),
        ("[title=Metadata]\n", "title=Macro", "Macro"),
        ("", "title=First,title=Last", "Last"),
        ("[title=First,title=Last]\n", "", "Last"),
        (".Line\n", "title=\"\"", ""),
    ] {
        let source = format!("{metadata}image::sample.svg[Alt,{attributes}]\n");
        let parsed = parse(&source, &Options::default())?;
        let [Block::Image(image)] = parsed.document().blocks.as_slice() else {
            return Err(format!("expected image: {source}").into());
        };
        assert!(
            matches!(image.title.as_ref(), [InlineNode::PlainText(text)] if text.content == expected),
            "{source}: {:?}",
            image.title
        );
        assert!(
            matches!(image.metadata.caption, Some(Caption::Numbered { number: Some(number), .. }) if number.get() == 1)
        );
        assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    }
    Ok(())
}

#[test]
fn selected_image_title_macros_register_once_with_source_locations() -> Result<(), Error> {
    for target in [
        "sample.svg",
        "dir/my image.svg",
        "https://example.org/image.svg",
    ] {
        for (container, closing) in [
            ("", ""),
            ("* Item\n+\n", ""),
            ("====\n", "\n===="),
            ("[cols=a]\n|===\na|\n", "\n|==="),
        ] {
            let source = format!(
                "= T\n:word: expanded\n\n{container}.Discarded footnote:[Unused.]\n[title=\"Also discarded footnote:[Unused too.]\"]\nimage::{target}[Alt,title=\"Éva {{word}} footnote:[Selected.]\"]{closing}\n\nBody footnote:[Body.]\n"
            );
            let parsed = parse(&source, &Options::default())?;
            let [selected, body] = parsed.document().footnotes.as_slice() else {
                return Err(
                    format!("expected two notes: {:?}", parsed.document().footnotes).into(),
                );
            };
            assert_eq!((selected.number, body.number), (1, 2));
            assert_eq!(
                source.get(selected.location.absolute_start..=selected.location.absolute_end),
                Some("footnote:[Selected.]")
            );
            assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
        }
    }
    Ok(())
}

#[test]
fn metadata_image_title_keeps_attribute_expansion_source_locations() -> Result<(), Error> {
    let source = "= T\n:word: expanded\n\n.Discarded footnote:[Unused.]\n[title=\"Éva {word} footnote:[Selected.]\"]\nimage::sample.svg[Alt]\n";
    let parsed = parse(source, &Options::default())?;
    let [note] = parsed.document().footnotes.as_slice() else {
        return Err("expected only the selected title's footnote".into());
    };
    assert_eq!(
        source.get(note.location.absolute_start..=note.location.absolute_end),
        Some("footnote:[Selected.]")
    );
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[test]
fn verbatim_image_syntax_keeps_the_preceding_title() -> Result<(), Error> {
    for style in ["listing", "literal", "source", "verse"] {
        let source = format!(".Line\n[{style}]\nimage::sample.svg[Alt,title=Macro]\n");
        let parsed = parse(&source, &Options::default())?;
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err(format!("expected styled paragraph: {style}").into());
        };
        assert!(
            matches!(paragraph.title.as_ref(), [InlineNode::PlainText(text)] if text.content == "Line")
        );
    }
    Ok(())
}

#[test]
fn inline_image_title_remains_a_tooltip() -> Result<(), Error> {
    let parsed = parse(
        "Inline image:sample.svg[Alt,title=Tooltip].",
        &Options::default(),
    )?;
    let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
        return Err("expected paragraph".into());
    };
    let image = paragraph
        .content
        .iter()
        .find_map(|node| {
            if let InlineNode::Macro(InlineMacro::Image(image)) = node {
                Some(image)
            } else {
                None
            }
        })
        .ok_or("expected inline image")?;
    assert_eq!(
        image
            .metadata
            .attributes
            .get("title")
            .map(ToString::to_string)
            .as_deref(),
        Some("Tooltip")
    );
    assert!(matches!(image.title.as_ref(), [InlineNode::PlainText(text)] if text.content == "Alt"));
    Ok(())
}
