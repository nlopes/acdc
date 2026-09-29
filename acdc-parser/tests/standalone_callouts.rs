use acdc_parser::{Block, Options, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn standalone_callouts_warn_at_the_unmatched_items() -> Result<(), Error> {
    // Diagnostics and their source spans are not part of document snapshots.
    let source = "<1> First.\n<2> Second.\n";
    let parsed = parse(source, &Options::default())?;
    let warnings = parsed.warnings();
    assert_eq!(warnings.len(), 2);
    for (warning, (number, text)) in warnings.iter().zip([(1, "<1> First."), (2, "<2> Second.")]) {
        assert_eq!(
            warning.kind.to_string(),
            format!("no callout found for <{number}>")
        );
        let location = &warning
            .location
            .as_ref()
            .ok_or("missing warning location")?
            .location;
        assert_eq!(
            source.get(location.absolute_start..=location.absolute_end),
            Some(text)
        );
    }
    Ok(())
}

#[test]
fn standalone_callouts_validate_source_markers_and_render_ordinals() -> Result<(), Error> {
    for marker in ["0", "2", "01", "999999999999999999999999999999999999"] {
        let source = format!("<{marker}> First.\n<.> Second.\n");
        let parsed = parse(&source, &Options::default())?;
        let [Block::CalloutList(list)] = parsed.document().blocks.as_slice() else {
            return Err(format!("expected a callout list for {source:?}").into());
        };
        assert_eq!(
            list.items
                .iter()
                .map(|item| item.callout.number)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        let messages = parsed
            .warnings()
            .iter()
            .map(|w| w.kind.to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            messages,
            [
                format!("callout list item index: expected 1, got {marker}"),
                "no callout found for <1>".to_owned(),
                "callout list item index: expected 2, got 1".to_owned(),
                "no callout found for <2>".to_owned(),
            ]
        );
    }
    Ok(())
}

#[test]
fn standalone_callouts_retain_pending_references_across_blocks() -> Result<(), Error> {
    let source = "----\nfirst <1>\n----\n\nProse.\n\n----\nsecond <2>\n----\n\n----\nNo markers.\n----\n<1> First.\n<2> Second.\n\n//-\n\n<1> Unmatched.\n";
    let parsed = parse(source, &Options::default())?;
    assert_eq!(parsed.warnings().len(), 1, "{:?}", parsed.warnings());
    let warning = parsed
        .warnings()
        .first()
        .ok_or("missing unmatched warning")?;
    assert_eq!(warning.kind.to_string(), "no callout found for <1>");
    let location = &warning
        .location
        .as_ref()
        .ok_or("missing warning location")?
        .location;
    assert_eq!(
        source.get(location.absolute_start..=location.absolute_end),
        Some("<1> Unmatched.")
    );
    Ok(())
}

#[test]
fn standalone_callouts_register_parent_notes_before_nested_notes() -> Result<(), Error> {
    for marker in ["*", ".", "Term::"] {
        let source = format!(
            "{marker} Parent footnote:[Parent.].\n<1> Child footnote:[Child.].\n<2> Last footnote:[Last.].\n"
        );
        let parsed = parse(&source, &Options::default())?;
        let notes = &parsed.document().footnotes;
        assert_eq!(notes.len(), 3, "{source}");
        for (note, expected) in notes.iter().zip([
            "footnote:[Parent.]",
            "footnote:[Child.]",
            "footnote:[Last.]",
        ]) {
            assert_eq!(
                source.get(note.location.absolute_start..=note.location.absolute_end),
                Some(expected),
                "{source}"
            );
        }
    }
    Ok(())
}

#[test]
fn standalone_callout_probes_do_not_register_paragraph_notes_twice() -> Result<(), Error> {
    let source = "Prose.\n<1> Literal marker footnote:[Only note.].\n";
    let parsed = parse(source, &Options::default())?;
    assert!(matches!(
        parsed.document().blocks.as_slice(),
        [Block::Paragraph(_)]
    ));
    assert_eq!(parsed.document().footnotes.len(), 1);
    assert!(parsed.warnings().is_empty());
    Ok(())
}
