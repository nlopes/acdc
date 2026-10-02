use acdc_parser::{Block, DocumentAttributeValue, DocumentAttributes, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn formatted_attributes_store_source_and_register_macros_at_use() -> Result<(), Error> {
    let parsed = parse(
        include_str!("../fixtures/tests/document_attribute_formatted.adoc"),
        &Options::default(),
    )?;
    let document = parsed.document();
    for (name, expected) in [
        ("bold", "*Bold*"),
        ("frozen", "*Early*"),
        ("quotes-first", "*Early*"),
        ("normal", "*Early* (C)"),
        ("link", "https://example.org[Site]"),
        ("empty-before", "**"),
        ("empty-after", "**"),
        ("escaped", "*{name}*"),
        ("q-c", "pass:q,c[*Escaped*]"),
    ] {
        assert_eq!(
            document
                .attributes
                .get(name)
                .and_then(DocumentAttributeValue::text),
            Some(expected),
            "{name}"
        );
        assert_eq!(
            serde_json::to_value(&document.attributes)?
                .get(name)
                .and_then(serde_json::Value::as_str),
            Some(expected),
            "{name}"
        );
    }
    assert!(!document.footnotes.iter().any(|note| note.id == Some("u")));
    assert_eq!(
        document
            .footnotes
            .iter()
            .filter(|note| note.id == Some("n"))
            .count(),
        1
    );
    assert!(
        document
            .footnotes
            .iter()
            .filter(|note| note.id.is_none())
            .count()
            > 1
    );
    Ok(())
}

#[test]
fn formatted_attributes_do_not_register_unused_definitions() -> Result<(), Error> {
    let parsed = parse(
        "= T\n:unused: pass:m[footnote:[Unused.] ((Unused)) anchor:unused[]]\n\nBody.\n",
        &Options::default(),
    )?;
    assert!(parsed.document().footnotes.is_empty());
    assert!(!parsed.document().references.contains_key("unused"));
    Ok(())
}

#[test]
fn formatted_attributes_register_footnotes_in_source_order() -> Result<(), Error> {
    for content in [
        "footnote:[Before.] {value} footnote:n[] footnote:[After.] {value}",
        "*footnote:[Before.] {value} footnote:n[] footnote:[After.] {value}*",
    ] {
        let source =
            format!("= T\n:value: pass:m[footnote:n[Named.] footnote:[Attribute.]]\n\n{content}\n");
        let parsed = parse(&source, &Options::default())?;
        assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
        let notes = &parsed.document().footnotes;
        assert_eq!(notes.len(), 5);
        for (note, expected) in
            notes
                .iter()
                .zip(["Before.", "Named.", "Attribute.", "After.", "Attribute."])
        {
            assert_eq!(
                note.content
                    .iter()
                    .filter_map(|node| {
                        if let InlineNode::PlainText(value) = node {
                            Some(value.content)
                        } else if let InlineNode::RawText(value) = node {
                            Some(value.content)
                        } else {
                            None
                        }
                    })
                    .collect::<String>(),
                expected
            );
        }
        for (_, note) in notes
            .iter()
            .enumerate()
            .filter(|(index, _)| matches!(index, 1 | 2 | 4))
        {
            assert_eq!(
                &parsed.source()[note.location.absolute_start..=note.location.absolute_end],
                "{value}"
            );
        }
    }
    Ok(())
}

#[test]
fn formatted_attributes_warn_only_on_used_conflicting_named_bodies() -> Result<(), Error> {
    let source = "= T\n:first: pass:m[footnote:n[First.]]\n:second: pass:m[footnote:n[Different.]]\n\n{first}\n";
    assert!(parse(source, &Options::default())?.warnings().is_empty());
    let source = format!("{source}\n{{second}}\n");
    let parsed = parse(&source, &Options::default())?;
    assert_eq!(parsed.document().footnotes.len(), 1);
    assert!(parsed.warnings().iter().any(|warning| matches!(
        warning.kind,
        acdc_parser::WarningKind::ConflictingFootnote { .. }
    )));
    Ok(())
}

#[test]
fn formatted_attributes_obey_locked_and_soft_caller_values() -> Result<(), Error> {
    let source = "= T\n:value: pass:m[footnote:[Rejected.] ((Rejected))]\n\n{value}\n";
    for options in [
        Options::with_attributes([("value", "Caller")])?,
        Options::builder().with_attribute("value", false).build()?,
    ] {
        let parsed = parse(source, &options)?;
        assert!(parsed.document().footnotes.is_empty());
    }
    let soft = Options::builder()
        .with_default_attribute("value", "Caller")
        .build()?;
    let parsed = parse(source, &soft)?;
    assert_eq!(parsed.document().footnotes.len(), 1);
    Ok(())
}

#[test]
fn formatted_attributes_keep_complete_reference_spans() -> Result<(), Error> {
    let source = "= T\n:value: pass:q[*Éva*]\n:alias: prefix {value} suffix\n\nα {alias} ω.\n";
    let parsed = parse(source, &Options::default())?;
    let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
        return Err("expected paragraph".into());
    };
    let value = paragraph
        .content
        .iter()
        .find_map(|node| {
            if let InlineNode::BoldText(value) = node {
                Some(value)
            } else {
                None
            }
        })
        .ok_or("missing bold")?;
    for location in
        std::iter::once(&value.location).chain(value.content.iter().map(InlineNode::location))
    {
        assert_eq!(
            &parsed.source()[location.absolute_start..=location.absolute_end],
            "{alias}"
        );
        assert_eq!((location.start.line, location.end.line), (5, 5));
    }
    Ok(())
}

#[test]
fn formatted_attributes_survive_owned_snapshots_and_compare_by_value() -> Result<(), Error> {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<DocumentAttributeValue<'static>>();
    assert_send_sync::<DocumentAttributes<'static>>();
    let attributes = {
        let parsed = parse(
            "= T\n:first: pass:q[*Éva*]\n:second: pass:q[*Éva*]\n\nBody.\n",
            &Options::default(),
        )?;
        assert_eq!(
            parsed.document().attributes.get("first"),
            parsed.document().attributes.get("second")
        );
        parsed.document().attributes.clone().into_static()
    };
    assert_eq!(
        attributes
            .get("first")
            .and_then(DocumentAttributeValue::text),
        Some("*Éva*")
    );
    let parsed = parse(
        "{first}\n",
        &Options::default().with_document_attributes(attributes),
    )?;
    let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
        return Err("expected paragraph".into());
    };
    assert!(matches!(
        paragraph.content.first(),
        Some(InlineNode::BoldText(_))
    ));
    Ok(())
}

#[test]
fn formatted_attributes_have_backend_independent_text_conditions_and_nodes() -> Result<(), Error> {
    let source = "= T\n:value: pass:q[*Bold*]\n:expected: pass:[*Bold*]\n:html: pass:[<strong>Bold</strong>]\n\nifeval::[\"{value}\" == \"{expected}\"]\nMatched.\nendif::[]\n\nifeval::[\"{value}\" == \"{html}\"]\nWrong.\nendif::[]\n\n{value}.\n";
    let mut expected = None;
    for backend in ["html", "pdf", "manpage"] {
        for newline in ["\n", "\r\n"] {
            let source = source.replace('\n', newline);
            let parsed = parse(
                &source,
                &Options::builder()
                    .with_attribute("backend", backend)
                    .build()?,
            )?;
            assert!(parsed.source().contains("Matched."));
            assert!(!parsed.source().contains("Wrong."));
            assert_eq!(
                parsed
                    .document()
                    .attributes
                    .get("value")
                    .and_then(DocumentAttributeValue::text),
                Some("*Bold*")
            );
            let structure = serde_json::to_value(&parsed.document().blocks)?;
            if newline == "\n" {
                if let Some(expected) = &expected {
                    assert_eq!(expected, &structure);
                } else {
                    expected = Some(structure);
                }
            }
        }
    }
    Ok(())
}

#[test]
fn formatted_attributes_warn_on_output_dependent_lists() -> Result<(), Error> {
    let source = "= T\n:bold: pass:q[*Bold*]\n:value: pass:q,c[*Bold*]\n:alias: pass:a,c[{bold}]\n:unknown: pass:typo[*Text*]\n\n{value} {alias} {unknown}\n";
    let parsed = parse(source, &Options::default())?;
    assert_eq!(
        parsed
            .document()
            .attributes
            .get("value")
            .and_then(DocumentAttributeValue::text),
        Some("pass:q,c[*Bold*]")
    );
    assert!(
        parsed
            .warnings()
            .iter()
            .any(|warning| warning.to_string().contains("backend markup"))
    );
    assert!(parsed.warnings().iter().any(|warning| {
        warning
            .to_string()
            .contains("unknown attribute-value substitution")
    }));
    assert!(parsed.warnings().iter().any(|warning| {
        warning
            .to_string()
            .contains("formatted references is unsupported")
    }));
    Ok(())
}

#[test]
fn formatted_attribute_warnings_follow_assignment_policy() -> Result<(), Error> {
    for value in [
        "pass:q,c[*Bold*]",
        "pass:m,c[https://example.org[Site]]",
        "pass:a,c[{bold}]",
        "pass:typo[*Unknown*]",
    ] {
        for prefix in ["", "\nBefore.\n\n"] {
            let source =
                format!("= T\n:bold: pass:q[*Bold*]\n{prefix}:value: {value}\n\n{{value}}\n");
            for options in [
                Options::default(),
                Options::builder()
                    .with_default_attribute("value", "Caller")
                    .build()?,
            ] {
                let parsed = parse(&source, &options)?;
                let [warning] = parsed.warnings() else {
                    return Err(format!("expected one warning: {:?}", parsed.warnings()).into());
                };
                assert!(matches!(warning.kind, acdc_parser::WarningKind::Other(_)));
                let location = warning
                    .source_location()
                    .ok_or("missing warning location")?;
                let line = source
                    .lines()
                    .position(|line| line.starts_with(":value:"))
                    .ok_or("missing declaration")?
                    + 1;
                assert_eq!(location.location.start.line, u32::try_from(line)?);
            }
            for options in [
                Options::with_attributes([("value", "Caller")])?,
                Options::builder().with_attribute("value", false).build()?,
            ] {
                let parsed = parse(&source, &options)?;
                assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
            }
        }
    }
    Ok(())
}

#[test]
fn formatted_attributes_use_source_text_in_include_paths() -> Result<(), Error> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests/includes");
    let source = "= T\n:chapter: pass:q[chapter]\n\ninclude::{chapter}.txt[]\n";
    let parsed = parse(source, &Options::builder().with_base_dir(path).build()?)?;
    assert!(parsed.source().contains("Profile include."));
    assert!(!parsed.source().contains("include::"));
    Ok(())
}

#[test]
fn formatted_attributes_preserve_missing_empty_escaped_and_nested_aliases() -> Result<(), Error> {
    let parsed = parse(
        include_str!("../fixtures/tests/document_attribute_profile_edges.adoc"),
        &Options::default(),
    )?;
    for (name, expected) in [
        ("empty", ""),
        ("missing", "*{missing}*"),
        ("escaped", "*{name}*"),
        ("frozen", "*Early*"),
        ("second", "before prefix *Éva* / _Italic_ suffix after"),
        ("raw-alias", "*Literal* / *Éva*"),
        (
            "explicit-alias",
            "https://example.org[Protected] before *Éva* / _Italic_ after",
        ),
        ("escaped-alias", "{bold}"),
        ("mixed-link", "https://example.org[Literal] / *Éva*"),
        ("bad-alias", "pass:a,c[{bold}]"),
    ] {
        assert_eq!(
            parsed
                .document()
                .attributes
                .get(name)
                .and_then(DocumentAttributeValue::text),
            Some(expected),
            "{name}"
        );
    }
    assert_eq!(parsed.document().footnotes.len(), 3);
    assert!(!parsed.document().references.contains_key("unused"));
    Ok(())
}
