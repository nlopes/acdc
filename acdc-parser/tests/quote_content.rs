use acdc_parser::{Block, CompatibilityMode, InlineNode, Options, SafeMode, parse, parse_file};

type Error = Box<dyn std::error::Error>;

fn plain_text(node: &InlineNode<'_>) -> Result<String, Error> {
    if let InlineNode::PlainText(text) = node {
        Ok(text.content.to_owned())
    } else if let InlineNode::RawText(text) = node {
        Ok(text.content.to_owned())
    } else {
        Err(format!("expected text, got {node:?}").into())
    }
}

#[rstest::rstest]
fn nested_references_use_the_selected_declaration_snapshot(
    #[values("quote", "verse")] style: &str,
    #[values(false, true)] named: bool,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let values = if named {
        "attribution='{word}',citetitle='{word}'"
    } else {
        "'{word}','{word}'"
    };
    let source = format!(
        ":other: Before\n:word: \\{{other}}\n\n[{style},{values}]\n:other: After\n____\nWords.\n____"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let metadata = parsed
        .document()
        .blocks
        .iter()
        .find_map(Block::metadata)
        .ok_or("missing quote")?;
    let expected = if mode == CompatibilityMode::Asciidoctor {
        "Before"
    } else {
        "{other}"
    };
    for content in [&metadata.attribution, &metadata.citetitle] {
        let node = content
            .as_ref()
            .and_then(|content| content.first())
            .ok_or("missing content")?;
        assert_eq!(plain_text(node)?, expected);
    }
    Ok(())
}

#[rstest::rstest]
fn body_substitutions_do_not_disable_quote_attribute_formatting(
    #[values("quote", "verse")] style: &str,
    #[values(false, true)] named: bool,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let values = if named {
        "attribution='*Ada*',citetitle='_Book_'"
    } else {
        "'*Ada*','_Book_'"
    };
    let source = format!("[{style},{values},subs=none]\n____\n*Literal body*.\n____");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let metadata = parsed
        .document()
        .blocks
        .iter()
        .find_map(Block::metadata)
        .ok_or("missing quote")?;
    assert!(matches!(
        metadata
            .attribution
            .as_ref()
            .and_then(|content| content.first()),
        Some(InlineNode::BoldText(_))
    ));
    assert!(matches!(
        metadata
            .citetitle
            .as_ref()
            .and_then(|content| content.first()),
        Some(InlineNode::ItalicText(_))
    ));
    Ok(())
}

#[rstest::rstest]
fn single_character_quote_content_keeps_its_authored_byte_span(
    #[values("x", "λ")] value: &str,
    #[values(false, true)] named: bool,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let values = if named {
        format!("attribution='{value}',citetitle='{value}'")
    } else {
        format!("'{value}','{value}'")
    };
    let source = format!("[quote,{values}]\n____\nWords.\n____");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let metadata = parsed
        .document()
        .blocks
        .iter()
        .find_map(Block::metadata)
        .ok_or("missing quote")?;
    let offsets = source.match_indices(value).map(|(offset, _)| offset);
    for (content, offset) in [&metadata.attribution, &metadata.citetitle]
        .into_iter()
        .zip(offsets)
    {
        let location = content
            .as_ref()
            .and_then(|content| content.first())
            .ok_or("missing content")?
            .location();
        assert_eq!(location.absolute_start, offset);
        assert_eq!(location.absolute_end, offset);
        assert_eq!(location.start, location.end);
    }
    Ok(())
}

#[rstest::rstest]
fn included_expanded_attribution_retains_the_reference_and_file(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests");
    let parsed = parse_file(
        root.join("compatibility_quote_locations.adoc"),
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let included = parsed
        .document()
        .blocks
        .iter()
        .filter_map(Block::metadata)
        .filter_map(|metadata| metadata.attribution.as_ref())
        .filter_map(|content| content.first())
        .find(|node| {
            parsed
                .source_location(node.location())
                .file
                .as_ref()
                .is_some_and(|file| file.ends_with("includes/compatibility_quote_locations.adoc"))
        })
        .ok_or("missing included attribution")?;
    let InlineNode::BoldText(bold) = included else {
        return Err("expected formatted attribution".into());
    };
    for node in std::iter::once(included).chain(bold.content.iter()) {
        let location = parsed.source_location(node.location());
        assert_eq!(
            location.file,
            Some(root.join("includes/compatibility_quote_locations.adoc"))
        );
        assert_eq!(location.location.start.line, 2);
        assert_eq!(location.location.end.line, 2);
        assert_eq!(location.location.start.column, 21);
        assert_eq!(location.location.end.column, 28);
    }
    assert_eq!(parsed.document().footnotes.len(), 2);
    Ok(())
}

#[rstest::rstest]
fn only_selected_quote_content_registers_footnotes(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests");
    let parsed = parse_file(
        root.join("compatibility_quote_snapshots.adoc"),
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let selected = if mode == CompatibilityMode::Asciidoctor {
        "position"
    } else {
        "named"
    };
    assert_eq!(
        parsed
            .document()
            .footnotes
            .iter()
            .map(|note| note.id)
            .collect::<Vec<_>>(),
        [
            Some(selected),
            Some("body"),
            Some("selected"),
            Some("citation")
        ],
    );
    Ok(())
}
