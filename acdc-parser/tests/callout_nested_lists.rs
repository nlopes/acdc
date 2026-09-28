use acdc_parser::{Block, Error as ParseError, Options, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn callout_continuations_propagate_invalid_child_errors() {
    // A parse error has no document snapshot; check both forms of attachment.
    let marker = ".".repeat(256);
    for child in ["", "* Nested.\n\n"] {
        let source =
            format!("----\ncode <1>\n----\n<1> Parent.\n{child}+\n{marker} Invalid depth.\n");
        assert!(
            matches!(
                parse(&source, &Options::default()),
                Err(ParseError::TryFromIntError(_))
            ),
            "invalid attached list must report its error for {source:?}"
        );
    }
}

#[test]
fn callout_nested_lists_register_footnotes_once_in_source_order() -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/callout_nested_lists.adoc");
    let parsed = parse(source, &Options::default())?;
    let notes = &parsed.document().footnotes;
    assert_eq!(notes.len(), 3);
    for (note, expected) in notes.iter().zip([
        "footnote:[Parent note.]",
        "footnote:[Child note.]",
        "footnote:[Last note.]",
    ]) {
        assert_eq!(
            source.get(note.location.absolute_start..=note.location.absolute_end),
            Some(expected)
        );
    }
    Ok(())
}

#[test]
fn callout_nested_contexts_preserve_parent_validation() -> Result<(), Error> {
    // Warning state is not part of the JSON fixtures.
    let source = include_str!("../fixtures/tests/callout_nested_contexts.adoc");
    let parsed = parse(source, &Options::default())?;
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[test]
fn callout_nested_lists_keep_exact_item_spans() -> Result<(), Error> {
    for child in ["* Nested.", ". Nested.", "Term:: Nested."] {
        let source = format!("----\ncode <1> <2>\n----\n<1> First.\n{child}\n<2> Second.\n");
        let parsed = parse(&source, &Options::default())?;
        let [_, Block::CalloutList(list)] = parsed.document().blocks.as_slice() else {
            return Err(format!("expected listing and callout list for {source:?}").into());
        };
        let [first, second] = list.items.as_slice() else {
            return Err("expected two callout items".into());
        };
        let first_source = source
            .get(first.location.absolute_start..=first.location.absolute_end)
            .ok_or("invalid first item span")?;
        assert_eq!(first_source, format!("<1> First.\n{child}"));
        let second_source = source
            .get(second.location.absolute_start..=second.location.absolute_end)
            .ok_or("invalid second item span")?;
        assert_eq!(second_source, "<2> Second.");
    }
    Ok(())
}
