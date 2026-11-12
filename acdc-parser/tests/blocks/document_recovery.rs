use acdc_parser::{
    ColumnStyle, ColumnWidth, CompatibilityMode, Error as ParseError, HorizontalAlignment, Options,
    VerticalAlignment, WarningKind,
    ast::{BlockKind, BlockRef, ColumnFormatRef, InlineKind, InlineRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn tab_separated_sibling_headings_keep_their_parent(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("=", "#")] marker: &str,
) -> Result<(), Error> {
    let source = format!(
        "{marker} Review\n\n{marker}{marker} Parent\n\n{marker}{marker}{marker} Child\n\n[[sibling]]\n{marker}{marker}\tSibling\n\nBody."
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [BlockKind::Section(parent), BlockKind::Section(sibling)] = &*parsed
        .document()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected two sibling sections".into());
    };
    assert_eq!(parent.level(), 1);
    assert_eq!(sibling.level(), 1);
    assert_eq!(sibling.blocks().count(), 1);
    let Some(BlockKind::Section(child)) = parent.blocks().next().map(BlockRef::kind) else {
        return Err("missing child".into());
    };
    assert_eq!(child.level(), 2);
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
fn mismatched_table_delimiters_continue_until_an_exact_close_or_eof(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("|", "!", ",", ":")] delimiter: &str,
    #[values(true, false)] terminated: bool,
) -> Result<(), Error> {
    let closing = if terminated {
        format!("\n{delimiter}===\n\nAfter.")
    } else {
        String::new()
    };
    let source = format!(
        "Before.\n\n{delimiter}===\nCell\n{delimiter}====\nStill inside.\n{delimiter}===suffix{closing}"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(
        parsed.document().blocks().count(),
        if terminated { 3 } else { 2 },
        "{:?}",
        parsed.document().blocks().collect::<Vec<_>>()
    );
    let Some(BlockKind::Table(table)) = parsed.document().blocks().nth(1).map(BlockRef::kind)
    else {
        return Err(format!(
            "expected recovered table: {:?}",
            parsed.document().blocks().collect::<Vec<_>>()
        )
        .into());
    };
    let span = table.location();
    let content = parsed
        .preprocessed_source()
        .get(span.absolute_start()..=span.absolute_end())
        .ok_or("invalid table span")?;
    assert!(content.contains(&format!(
        "{delimiter}====\nStill inside.\n{delimiter}===suffix"
    )));
    assert_eq!(table.close_delimiter_location().is_some(), terminated);
    assert_eq!(
        parsed
            .warnings()
            .iter()
            .any(|w| matches!(w.kind, WarningKind::UnterminatedTable { .. })),
        !terminated
    );
    if !terminated {
        assert!(parsed.source_recovery().is_some());
    }
    Ok(())
}

#[rstest::rstest]
fn excessive_list_depth_is_literal_and_located(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(".", "*")] marker: &str,
) -> Result<(), Error> {
    let item = format!("{} *Unformatted* item.", marker.repeat(256));
    let source = format!("Before.\n\n{item}\n\n* Normal item.\n\nAfter.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [
        BlockKind::Paragraph(_),
        BlockKind::Paragraph(literal),
        BlockKind::UnorderedList(_),
        BlockKind::Paragraph(_),
    ] = &*parsed
        .document()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected recovered item and unaffected surrounding blocks".into());
    };
    assert_eq!(literal.source_text(), Some(item.as_str()));
    let Some(InlineKind::Plain(text)) = literal.content().inlines().next().map(InlineRef::kind)
    else {
        return Err("expected literal inline".into());
    };
    assert_eq!(text.text(), item);
    let warning = parsed.source_recovery().ok_or("missing depth warning")?;
    assert_eq!(
        warning
            .source_location()
            .ok_or("missing depth location")?
            .location
            .start
            .line,
        3
    );
    Ok(())
}

#[rstest::rstest]
fn oversized_column_width_uses_one_and_keeps_other_controls(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[cols=\"2*<.>4294967296a,2\"]\n|===\n|A |B |C\n|===",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(BlockKind::Table(table)) = parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("expected table".into());
    };
    assert_eq!(
        table
            .columns()
            .map(ColumnFormatRef::width)
            .collect::<Vec<_>>(),
        [
            ColumnWidth::Proportional(1),
            ColumnWidth::Proportional(1),
            ColumnWidth::Proportional(2)
        ]
    );
    let column = table.columns().next().ok_or("missing recovered column")?;
    assert_eq!(column.halign(), HorizontalAlignment::Left);
    assert_eq!(column.valign(), VerticalAlignment::Bottom);
    assert_eq!(column.style(), ColumnStyle::AsciiDoc);
    assert!(parsed.source_recovery().is_some());
    Ok(())
}

#[rstest::rstest]
fn manpage_title_fallback_warns_while_strict_mode_still_rejects_it(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "= Invalid title\n:doctype: manpage\n\n== NAME\n\ncommand - purpose";
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    let parsed = parse(source, &options)?;
    assert!(parsed.source_recovery().is_none());
    let warning = parsed
        .warnings()
        .iter()
        .find(|w| matches!(w.kind, WarningKind::NonConformingManpageTitle { .. }))
        .ok_or("missing manpage title warning")?;
    assert_eq!(
        warning
            .source_location()
            .ok_or("missing title location")?
            .location
            .start
            .line,
        1
    );
    assert_eq!(
        parsed
            .document()
            .attributes()
            .get("manvolnum")
            .and_then(|value| value.as_str()),
        Some("1")
    );
    assert!(matches!(
        parse(
            source,
            &Options::builder()
                .with_compatibility_mode(mode)
                .with_strict()
                .build()?
        ),
        Err(ParseError::NonConformingManpageTitle(..))
    ));
    Ok(())
}
