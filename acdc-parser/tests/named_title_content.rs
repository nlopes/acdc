use acdc_parser::{Block, CompatibilityMode, InlineNode, Options, SafeMode, parse, parse_file};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn title_passthroughs_keep_the_authored_line(
    #[values("++{word}++", "pass:q[{word}]")] value: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!(":word: *Bold*\n\n[title='{value}']\nText.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(Block::Paragraph(block)) = parsed.document().blocks.last() else {
        return Err("missing paragraph".into());
    };
    for node in &block.title {
        let location = node.location();
        assert_eq!(location.start.line, 3);
        assert_eq!(location.end.line, 3);
        assert!(location.start.column >= 9);
        assert!(location.end.column <= u32::try_from(8 + value.len())?);
    }
    Ok(())
}

#[rstest::rstest]
fn explicit_passthrough_expansion_keeps_the_title_snapshot(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        ":inner: Before\n:outer: \\{inner}\n\n[title='pass:a[{outer}]']\n:inner: After\nText.",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(Block::Paragraph(block)) = parsed.document().blocks.last() else {
        return Err("missing paragraph".into());
    };
    assert!(
        matches!(block.title.as_ref(), [InlineNode::RawText(text)] if text.content == "Before")
    );
    Ok(())
}

#[rstest::rstest]
fn authored_single_character_title_content_excludes_its_delimiters(
    #[values("*x*", "*é*", "x", "é")] value: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("[title='{value}']\nText.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(Block::Paragraph(block)) = parsed.document().blocks.last() else {
        return Err("missing paragraph".into());
    };
    let outer = block.title.first().ok_or("missing title")?;
    let (inner, content) = if let InlineNode::BoldText(bold) = outer {
        (
            bold.content.first().ok_or("missing bold content")?,
            value.trim_matches('*'),
        )
    } else {
        (outer, value)
    };
    let location = inner.location();
    let start = source.find(content).ok_or("missing authored content")?;
    assert_eq!(location.absolute_start, start);
    assert_eq!(location.absolute_end, start);
    assert_eq!(location.start, location.end);
    Ok(())
}

#[rstest::rstest]
fn named_titles_interpret_the_expanded_value(
    #[values("'", "\"", "")] quote: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!(":word: *Bold*\n\n[title={quote}{{word}}{quote}]\nText.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(Block::Paragraph(block)) = parsed.document().blocks.last() else {
        return Err("missing paragraph".into());
    };
    if mode == CompatibilityMode::Draft && quote != "'" {
        assert!(
            matches!(block.title.as_ref(), [InlineNode::RawText(text)] if text.content == "*Bold*")
        );
    } else {
        assert!(matches!(block.title.as_ref(), [InlineNode::BoldText(_)]));
    }
    Ok(())
}

#[rstest::rstest]
fn expanded_title_nodes_keep_the_full_authored_reference(
    #[values("*é*", "*Longer text*", "x")] value: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!(":word: {value}\n\n[title='{{word}}']\nText.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(Block::Paragraph(block)) = parsed.document().blocks.last() else {
        return Err("missing paragraph".into());
    };
    let node = block.title.first().ok_or("missing title")?;
    let start = source.find("{word}").ok_or("missing reference")?;
    for node in std::iter::once(node).chain(if let InlineNode::BoldText(bold) = node {
        bold.content.iter()
    } else {
        [].iter()
    }) {
        let location = node.location();
        assert_eq!(location.absolute_start, start);
        assert_eq!(location.absolute_end, start + "{word}".len() - 1);
        assert_eq!(location.start.line, 3);
        assert_eq!(location.start.column, 9);
        assert_eq!(location.end.column, 14);
    }
    Ok(())
}

#[rstest::rstest]
fn passthroughs_protect_the_value_after_list_expansion(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        ":word: *Bold*\n\n[title='++{word}++']\nText.",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(Block::Paragraph(block)) = parsed.document().blocks.last() else {
        return Err("missing paragraph".into());
    };
    assert!(
        matches!(block.title.as_ref(), [InlineNode::RawText(text)] if text.content == "*Bold*")
    );
    Ok(())
}

#[rstest::rstest]
fn only_selected_named_titles_register_footnotes(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        include_str!("../fixtures/tests/compatibility_title_snapshots.adoc"),
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(
        parsed.document().footnotes.len(),
        if mode == CompatibilityMode::Asciidoctor {
            4
        } else {
            3
        }
    );
    Ok(())
}

#[rstest::rstest]
fn selected_include_titles_keep_their_source_file(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests");
    let parsed = parse_file(
        root.join("compatibility_title_locations.adoc"),
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let title = parsed
        .document()
        .blocks
        .iter()
        .filter_map(|block| {
            if let Block::Paragraph(paragraph) = block {
                Some(&paragraph.title)
            } else {
                None
            }
        })
        .find(|title| title.iter().any(|node| {
            matches!(node, InlineNode::Macro(acdc_parser::InlineMacro::Footnote(note)) if note.id == Some("included"))
        }))
        .ok_or("missing included title")?;
    let bold = title
        .iter()
        .find(|node| matches!(node, InlineNode::BoldText(_)))
        .ok_or("missing expanded bold text")?;
    let origin = parsed.source_location(bold.location());
    assert_eq!(
        origin.file,
        Some(root.join("includes/compatibility_title_locations.adoc"))
    );
    assert_eq!(origin.location.start.line, 2);
    assert_eq!(origin.location.end.line, 2);
    assert_eq!(origin.location.end.column - origin.location.start.column, 5);
    assert_eq!(parsed.document().footnotes.len(), 2);
    Ok(())
}

#[rstest::rstest]
fn expanded_title_notes_keep_the_first_definition_and_authored_warnings(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":first: footnote:shared[First ((Selected)) note.]\n:second: footnote:shared[Different note.]\n\n[title='footnote:[Discarded.] ((Discarded))']\n[title='{first}']\nText.\n\n[title='{second}']\nText.";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [note] = parsed.document().footnotes.as_slice() else {
        return Err("expected one selected note".into());
    };
    assert_eq!(note.number, 1);
    assert_eq!(
        source.get(note.location.absolute_start..=note.location.absolute_end),
        Some("{first}")
    );
    assert_eq!(
        note.content
            .iter()
            .filter(|node| matches!(
                node,
                InlineNode::Macro(acdc_parser::InlineMacro::IndexTerm(_))
            ))
            .count(),
        1
    );
    let [warning] = parsed.warnings() else {
        return Err("expected one conflicting-footnote warning".into());
    };
    let acdc_parser::WarningKind::ConflictingFootnote { first, .. } = &warning.kind else {
        return Err("expected conflicting footnote".into());
    };
    assert_eq!(first.location.start.line, 5);
    assert_eq!(first.location.start.column, 9);
    assert_eq!(first.location.end.column, 15);
    let location = warning
        .location
        .as_ref()
        .ok_or("missing warning location")?;
    assert_eq!(location.location.start.line, 8);
    assert_eq!(location.location.start.column, 9);
    assert_eq!(location.location.end.column, 16);
    Ok(())
}
