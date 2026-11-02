use acdc_parser::{Block, CompatibilityMode, InlineNode, Options, WarningKind, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn unsupported_passthroughs_keep_source_without_registering_macros(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("q,c", "m,c", "q,verbatim", "callouts", "typo")] substitutions: &str,
) -> Result<(), Error> {
    let full =
        format!("pass:{substitutions}[<mark> *Bold* footnote:n[Unused.\\] anchor:unused[\\]]");
    let source = format!("= T\n\nEarlier.\n\n{full}\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let document = parsed.document();
    assert_eq!(document.footnotes, []);
    assert!(!document.references.contains_key("unused"));
    let Some(Block::Paragraph(paragraph)) = document.blocks.last() else {
        return Err("missing recovered paragraph".into());
    };
    let [InlineNode::RawText(raw)] = paragraph.content.as_slice() else {
        return Err(format!("expected protected source: {:?}", paragraph.content).into());
    };
    assert_eq!(raw.content, full);
    assert_eq!(
        source.get(raw.location.absolute_start..=raw.location.absolute_end),
        Some(full.as_str())
    );
    let [warning] = parsed.warnings() else {
        return Err(format!("expected one recovery warning: {:?}", parsed.warnings()).into());
    };
    assert!(matches!(warning.kind, WarningKind::ContentRecovery { .. }));
    let location = warning
        .source_location()
        .ok_or("missing warning location")?;
    assert_eq!(location.location.start.line, 5);
    assert_eq!(location.location.start.column, 1);
    Ok(())
}

#[rstest::rstest]
fn passthrough_recovery_keeps_bracket_escapes_and_later_macro_locations(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "Earlier.\n\ncafé pass:q,c[α *Éva* \\] tail] footnote:n[Kept.]\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.last() else {
        return Err("missing paragraph".into());
    };
    let raw = paragraph
        .content
        .iter()
        .find_map(|node| {
            let InlineNode::RawText(raw) = node else {
                return None;
            };
            Some(raw)
        })
        .ok_or("missing recovered source")?;
    assert_eq!(raw.content, "pass:q,c[α *Éva* \\] tail]");
    assert_eq!(
        source.get(raw.location.absolute_start..=raw.location.absolute_end),
        Some(raw.content)
    );
    let [note] = parsed.document().footnotes.as_slice() else {
        return Err("expected only the later footnote".into());
    };
    assert_eq!(
        source.get(note.location.absolute_start..=note.location.absolute_end),
        Some("footnote:n[Kept.]")
    );
    assert_eq!(parsed.warnings().len(), 1);
    Ok(())
}

#[rstest::rstest]
fn unsupported_attribute_values_use_the_same_recovery_category(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        include_str!("../fixtures/tests/compatibility_passthrough_recovery.adoc"),
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.document().footnotes.len(), 1);
    assert!(!parsed.document().references.contains_key("unused"));
    assert_eq!(parsed.warnings().len(), 10);
    assert!(
        parsed
            .warnings()
            .iter()
            .all(|warning| matches!(warning.kind, WarningKind::ContentRecovery { .. }))
    );
    Ok(())
}
