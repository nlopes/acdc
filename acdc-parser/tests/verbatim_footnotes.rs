use acdc_parser::{InlineNode, Options, WarningKind, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn xref_nested_footnotes_register_once_and_keep_source_locations() -> Result<(), Error> {
    // The footnote catalog, numbering, and warnings are absent from parser snapshots.
    let source = include_str!("../fixtures/tests/xref_nested_footnotes.adoc");
    let parsed = parse(source, &Options::default())?;
    let notes = &parsed.document().footnotes;
    assert_eq!(notes.len(), 13);
    for (index, note) in notes.iter().enumerate() {
        assert_eq!(note.number as usize, index + 1);
        let span = source
            .get(note.location.absolute_start..=note.location.absolute_end)
            .ok_or("invalid footnote span")?;
        assert!(
            span.starts_with("footnote:") && span.ends_with(']'),
            "{span:?}"
        );
        assert!(
            !span.contains("Ignored replacement") && !span.contains("Escaped"),
            "{span:?}"
        );
    }
    assert_eq!(parsed.warnings().iter().filter(|warning| {
        matches!(&warning.kind, WarningKind::ConflictingFootnote { id, .. } if id == "shared")
    }).count(), 1);
    Ok(())
}

#[test]
fn nested_footnote_catalog_locations_are_document_absolute() -> Result<(), Error> {
    for body in [
        "*Bold footnote:[Note {name}.]*",
        "link:https://example.org[Label footnote:[Note {name}.]]",
        "((Term footnote:[Note {name}.]))",
        "xref:target[Label footnote:[Note {name}.] after]",
        r#"xref:target["Éva \"quoted\" footnote:[Note {name}.] after",role=hot]"#,
    ] {
        let source = format!("= Locations\n:name: Éva\n\nPrefix {body}.\n");
        let parsed = parse(&source, &Options::default())?;
        let [note] = parsed.document().footnotes.as_slice() else {
            return Err(format!("expected one note: {body}").into());
        };
        assert_eq!(
            source.get(note.location.absolute_start..=note.location.absolute_end),
            Some("footnote:[Note {name}.]"),
            "{body}"
        );
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn verbatim_footnotes_keep_one_definition_and_original_locations() -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/subs_verbatim_footnotes.adoc");
    let parsed = parse(source, &Options::default())?;
    let notes = &parsed.document().footnotes;
    assert_eq!(notes.len(), 14);
    for (index, note) in notes.iter().enumerate() {
        assert_eq!(note.number as usize, index + 1);
        let span = source
            .get(note.location.absolute_start..=note.location.absolute_end)
            .ok_or("invalid footnote span")?;
        assert!(
            span.starts_with("footnote:") && span.ends_with(']'),
            "{span:?}"
        );
        assert!(
            !span.contains("Ignored") && !span.contains("Disabled"),
            "{span:?}"
        );
    }
    Ok(())
}

#[test]
fn footnote_reuse_and_missing_references_do_not_consume_numbers() -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/footnote_reuse.adoc");
    let parsed = parse(source, &Options::default())?;
    let notes = &parsed.document().footnotes;
    assert_eq!(notes.len(), 4);
    for (note, expected) in notes.iter().zip([
        "footnote:shared[Shared body.]",
        "footnote:[ ]",
        "footnote:missing[Later definition.]",
        "footnote:[Last body.]",
    ]) {
        assert_eq!(
            source.get(note.location.absolute_start..=note.location.absolute_end),
            Some(expected)
        );
    }
    let [first, blank, later, last] = notes.as_slice() else {
        return Err("missing definitions".into());
    };
    assert_eq!(
        [first.number, blank.number, later.number, last.number],
        [1, 2, 3, 4]
    );
    assert!(
        matches!(first.content.as_slice(), [InlineNode::PlainText(text)] if text.content == "Shared body.")
    );
    assert!(parsed.warnings().iter().any(|warning| {
        warning
            .to_string()
            .contains("invalid footnote reference: missing")
    }));
    Ok(())
}
