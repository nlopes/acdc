use acdc_parser::{
    Block, Caption, CaptionKind, CompatibilityMode, InlineNode, Options, SafeMode, parse,
    parse_file,
};

type Error = Box<dyn std::error::Error>;

fn text<'a>(node: &InlineNode<'a>) -> Result<&'a str, Error> {
    if let InlineNode::PlainText(plain) = node {
        Ok(plain.content)
    } else if let InlineNode::RawText(raw) = node {
        Ok(raw.content)
    } else {
        Err(format!("expected text, got {node:?}").into())
    }
}

#[rstest::rstest]
fn nested_caption_references_use_the_declaration_snapshot(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":other: Before\n:word: \\{other}\n\n.Title\n[caption='{word}']\n:other: After\n====\nBody.\n====";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let node = parsed
        .document()
        .blocks
        .iter()
        .filter_map(Block::metadata)
        .find_map(|metadata| {
            metadata
                .caption_content()
                .and_then(|content| content.first())
        })
        .ok_or("missing caption")?;
    assert_eq!(
        text(node)?,
        if mode == CompatibilityMode::Asciidoctor {
            "Before"
        } else {
            "{other}"
        }
    );
    Ok(())
}

#[rstest::rstest]
fn introduced_caption_markup_is_parsed_independently_of_body_substitutions(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":prefix: *Prefix*\n\n.Title\n[caption='{prefix}',subs=none]\n====\n*Body*.\n====";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let node = parsed
        .document()
        .blocks
        .first()
        .and_then(Block::metadata)
        .and_then(|metadata| metadata.caption_content())
        .and_then(|content| content.first())
        .ok_or("missing caption")?;
    let InlineNode::BoldText(bold) = node else {
        return Err(format!("expected bold caption: {node:?}").into());
    };
    assert_eq!(
        text(bold.content.first().ok_or("missing bold text")?)?,
        "Prefix"
    );
    Ok(())
}

#[rstest::rstest]
fn a_single_character_caption_keeps_its_authored_byte_span(
    #[values("x", "λ")] value: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!(".Title\n[caption='{value}']\n====\nBody.\n====");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let node = parsed
        .document()
        .blocks
        .first()
        .and_then(Block::metadata)
        .and_then(|metadata| metadata.caption_content())
        .and_then(|content| content.first())
        .ok_or("missing caption")?;
    let start = source.find(value).ok_or("missing authored character")?;
    assert_eq!(node.location().absolute_start, start);
    assert_eq!(node.location().absolute_end, start);
    assert_eq!(node.location().start, node.location().end);
    Ok(())
}

#[rstest::rstest]
fn included_caption_nodes_keep_the_authored_reference_and_file(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests");
    let parsed = parse_file(
        root.join("compatibility_caption_locations.adoc"),
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let node = parsed
        .document()
        .blocks
        .iter()
        .filter_map(Block::metadata)
        .filter_map(|metadata| metadata.caption_content())
        .filter_map(|content| content.first())
        .find(|node| {
            parsed
                .source_location(node.location())
                .file
                .as_ref()
                .is_some_and(|file| file.ends_with("includes/compatibility_caption_locations.adoc"))
        })
        .ok_or("missing included caption")?;
    let InlineNode::BoldText(bold) = node else {
        return Err("expected bold included caption".into());
    };
    for node in std::iter::once(node).chain(bold.content.iter()) {
        let source = parsed.source_location(node.location());
        assert_eq!(
            source.file,
            Some(root.join("includes/compatibility_caption_locations.adoc"))
        );
        assert_eq!(
            (source.location.start.line, source.location.end.line),
            (3, 3)
        );
        assert_eq!(
            (source.location.start.column, source.location.end.column),
            (11, 18)
        );
    }
    assert_eq!(parsed.document().footnotes.len(), 2);
    Ok(())
}

#[rstest::rstest]
fn selected_caption_and_alt_content_registers_footnotes_in_source_order(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests");
    let parsed = parse_file(
        root.join("compatibility_caption_snapshots.adoc"),
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(
        parsed
            .document()
            .footnotes
            .iter()
            .map(|note| note.id)
            .collect::<Vec<_>>(),
        [
            Some("selected"),
            Some("body"),
            Some("caption_first"),
            Some("alt_second"),
            Some("alt_first"),
            Some("caption_second")
        ]
    );
    Ok(())
}

#[rstest::rstest]
fn custom_and_empty_captions_leave_each_kind_numbering_unchanged(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("example", "figure", "table", "listing")] kind: &str,
) -> Result<(), Error> {
    let (body, expected) = match kind {
        "example" => ("====\nBody.\n====", CaptionKind::Example),
        "figure" => ("image::photo.png[Photo]", CaptionKind::Figure),
        "table" => ("|===\n|Cell\n|===", CaptionKind::Table),
        "listing" => ("----\ncode\n----", CaptionKind::Listing),
        _ => return Err("unexpected caption kind".into()),
    };
    let source = format!(
        ":{kind}-caption: Label\n\n.Custom\n[caption='*Prefix*: ']\n{body}\n\n.Empty\n[caption='']\n{body}\n\n.Numbered\n{body}\n\n.Next\n{body}"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let captions = parsed
        .document()
        .blocks
        .iter()
        .filter_map(Block::metadata)
        .filter_map(|metadata| metadata.caption.as_ref())
        .collect::<Vec<_>>();
    let [custom, empty, first, second] = captions.as_slice() else {
        return Err("expected four captions".into());
    };
    assert!(matches!(custom, Caption::CustomContent(content) if !content.is_empty()));
    assert!(matches!(empty, Caption::CustomContent(content) if content.is_empty()));
    for (caption, ordinal) in [(first, 1), (second, 2)] {
        assert!(
            matches!(caption, Caption::Numbered {kind, number:Some(number), ..} if *kind == expected && number.get() == ordinal)
        );
    }
    assert_eq!(parsed.document().highest_caption_number(expected), 2);
    Ok(())
}

#[rstest::rstest]
fn cross_reference_targets_retain_the_selected_caption_content(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":prefix: *Prefix*\n\n[#target]\n.Title\n[caption='{prefix}: ']\n====\nBody.\n====\n\nSee <<target>>.";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let document = parsed.document();
    let metadata = document
        .blocks
        .first()
        .and_then(Block::metadata)
        .ok_or("missing block")?;
    let reference = document
        .references
        .get("target")
        .ok_or("missing reference target")?;
    assert_eq!(reference.caption, metadata.caption);
    let Some(Caption::CustomContent(content)) = &reference.caption else {
        return Err("missing custom reference caption".into());
    };
    assert!(matches!(content.first(), Some(InlineNode::BoldText(_))));
    assert!(!metadata.attributes.contains_key("caption"));
    Ok(())
}

#[rstest::rstest]
fn expanded_caption_and_alt_keep_attribute_order_when_source_spans_coincide(
    #[values(false, true)] caption_first: bool,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let caption = "caption='Caption footnote:caption[Caption note]: '";
    let alt = "alt='Alt footnote:alt[Alt note]'";
    let entries = if caption_first {
        format!("{caption},{alt}")
    } else {
        format!("{alt},{caption}")
    };
    let source = format!(":media: {entries}\n\n.Title\n[{{media}}]\nimage::photo.png[]");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let expected = if caption_first {
        [Some("caption"), Some("alt")]
    } else {
        [Some("alt"), Some("caption")]
    };
    assert_eq!(
        parsed
            .document()
            .footnotes
            .iter()
            .map(|note| note.id)
            .collect::<Vec<_>>(),
        expected
    );
    let content = parsed
        .document()
        .blocks
        .first()
        .and_then(Block::metadata)
        .and_then(|metadata| metadata.caption_content())
        .ok_or("missing caption")?;
    let start = source
        .find("{media}")
        .ok_or("missing attribute reference")?;
    for node in content {
        assert_eq!(
            (node.location().absolute_start, node.location().absolute_end),
            (start, start + "{media}".len() - 1)
        );
    }
    Ok(())
}
