use acdc_parser::{DocumentAttributeValue, Options, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn header_metadata_attributes_follow_source_order() -> Result<(), Error> {
    let parsed = parse(
        include_str!("../fixtures/tests/header_metadata_attributes.adoc"),
        &Options::default(),
    )?;
    let attributes = &parsed.document().attributes;
    for (name, expected) in [
        ("author", "Alice Smith"),
        ("email", "updated@example.org"),
        ("captured-email", "alice@example.org"),
        ("writer", "Later Writer"),
        ("captured-author", "Alice Smith"),
        ("captured-firstname", "Alice"),
        ("revnumber", "2.5"),
        ("captured-revision", "2.5"),
        ("revremark", "Reviewed by Alice Smith"),
    ] {
        assert_eq!(
            attributes.get(name).and_then(DocumentAttributeValue::text),
            Some(expected),
            "{name}"
        );
    }
    let header = parsed.document().header.as_ref().ok_or("missing header")?;
    let author = header.authors.first().ok_or("missing author")?;
    assert_eq!(author.first_name, "Alice");
    assert_eq!(author.last_name, "Smith");
    assert_eq!(author.email, Some("updated@example.org"));
    let parsed = parse(
        "= T\n:writer: Alice\n{writer} Smith\n:writer: Later\nv1.0, 2026-10-04",
        &Options::default(),
    )?;
    assert_eq!(
        parsed
            .document()
            .attributes
            .get("author")
            .and_then(DocumentAttributeValue::text),
        Some("Alice Smith")
    );
    Ok(())
}

#[test]
fn header_metadata_multiple_authors_survive_interleaved_entries() -> Result<(), Error> {
    for ending in ["", "\n", "\r\n"] {
        let source = format!(
            "= T\n:kept: header\nÉlodie Durand <elodie@example.org>; José García <jose@example.org>\n:captured: {{author_2}}\n////\n\n:kept: hidden\n////\nv1.0, 2026-10-04{ending}"
        );
        let parsed = parse(&source, &Options::default())?;
        let header = parsed.document().header.as_ref().ok_or("missing header")?;
        assert_eq!(header.authors.len(), 2);
        let attributes = &parsed.document().attributes;
        for (name, expected) in [
            ("captured", "José García"),
            ("authorcount", "2"),
            ("authors", "Élodie Durand, José García"),
            ("kept", "header"),
            ("revnumber", "1.0"),
        ] {
            assert_eq!(
                attributes.get(name).and_then(DocumentAttributeValue::text),
                Some(expected),
                "{name}"
            );
        }
        assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    }
    Ok(())
}

#[test]
fn header_metadata_explicit_fields_override_derived_values() -> Result<(), Error> {
    let source = "= T\n:kept: header\nAlice Smith\n:captured: {author}\n:author: Explicit Writer\n:firstname: Chosen\nv1.0, 2026-10-04\n\nBody";
    let parsed = parse(source, &Options::default())?;
    let attributes = &parsed.document().attributes;
    for (name, expected) in [
        ("captured", "Alice Smith"),
        ("author", "Explicit Writer"),
        ("authors", "Explicit Writer"),
        ("firstname", "Chosen"),
        ("lastname", "Writer"),
    ] {
        assert_eq!(
            attributes.get(name).and_then(DocumentAttributeValue::text),
            Some(expected),
            "{name}"
        );
    }
    let header = parsed.document().header.as_ref().ok_or("missing header")?;
    let author = header.authors.first().ok_or("missing author")?;
    assert_eq!(author.first_name, "Explicit");
    assert_eq!(author.last_name, "Writer");
    Ok(())
}

#[test]
fn header_metadata_entries_respect_locked_and_soft_caller_values() -> Result<(), Error> {
    let source = "= T\n:kept: header\nAlice Smith\n:captured-author: {author}\n:captured-firstname: {firstname}\n:author: Document Writer\n:firstname: Document\n:revnumber: 9\nv1.0, 2026-10-04\n\nBody";
    for locked in [false, true] {
        let builder = Options::builder();
        let options = if locked {
            builder
                .with_attribute("author", "Caller Writer")
                .with_attribute("firstname", "Caller")
                .with_attribute("revnumber", "7")
                .build()?
        } else {
            builder
                .with_default_attribute("author", "Caller Writer")
                .with_default_attribute("firstname", "Caller")
                .with_default_attribute("revnumber", "7")
                .build()?
        };
        let parsed = parse(source, &options)?;
        let attributes = &parsed.document().attributes;
        for (name, expected) in [
            ("captured-author", "Caller Writer"),
            ("captured-firstname", "Caller"),
            (
                "author",
                if locked {
                    "Caller Writer"
                } else {
                    "Document Writer"
                },
            ),
            ("firstname", if locked { "Caller" } else { "Document" }),
            ("revnumber", if locked { "7" } else { "9" }),
        ] {
            assert_eq!(
                attributes.get(name).and_then(DocumentAttributeValue::text),
                Some(expected),
                "{name}, locked={locked}"
            );
        }
    }
    let options = Options::builder()
        .with_default_attribute("firstname", "Prefilled")
        .build()?;
    let parsed = parse(
        "= T\n:kept: header\nAlice Smith\n:capture: {firstname}\nv1.0, 2026-10-04",
        &options,
    )?;
    for name in ["firstname", "capture"] {
        assert_eq!(
            parsed
                .document()
                .attributes
                .get(name)
                .and_then(DocumentAttributeValue::text),
            Some("Prefilled")
        );
    }
    Ok(())
}

#[test]
fn header_metadata_blank_lines_end_optional_slots() -> Result<(), Error> {
    for (source, authors) in [
        ("= T\n:kept: header\n\nCorrect Author\nv1.0, 2026-10-04", 0),
        (
            "= T\n:kept: header\n \t\nCorrect Author\nv1.0, 2026-10-04",
            0,
        ),
        ("= T\nCorrect Author\n:kept: header\n\nv1.0, 2026-10-04", 1),
    ] {
        let parsed = parse(source, &Options::default())?;
        let header = parsed.document().header.as_ref().ok_or("missing header")?;
        assert_eq!(header.authors.len(), authors);
        assert_eq!(parsed.document().attributes.get("revnumber"), None);
        assert!(!parsed.document().blocks.is_empty());
    }
    Ok(())
}

#[test]
fn header_metadata_without_optional_lines_applies_entries_once() -> Result<(), Error> {
    for suffix in ["", "\n", "\n\nBody"] {
        let source = format!("= T\n:chain: root\n////\n////\n:chain: {{chain}}-next{suffix}");
        let parsed = parse(&source, &Options::default())?;
        assert_eq!(
            parsed
                .document()
                .attributes
                .get("chain")
                .and_then(DocumentAttributeValue::text),
            Some("root-next")
        );
        let header = parsed.document().header.as_ref().ok_or("missing header")?;
        assert!(header.authors.is_empty());
    }
    let source = "= T\n:kept: café";
    let parsed = parse(source, &Options::default())?;
    let header = parsed.document().header.as_ref().ok_or("missing header")?;
    assert_eq!(header.location.end.line, 2);
    assert_eq!(header.location.end.column, 11);
    assert_eq!(
        header.location.absolute_end,
        source.char_indices().next_back().ok_or("empty source")?.0
    );
    Ok(())
}

#[cfg(feature = "setext")]
#[test]
fn header_metadata_setext_titles_keep_interleaved_slots() -> Result<(), Error> {
    let source = "Metadata Title\n==============\n:kept: header\nAlice Smith\n:capture: {author}\nv1.0, 2026-10-04\n\nBody";
    let options = Options::builder().with_setext().build()?;
    let parsed = parse(source, &options)?;
    for (name, expected) in [
        ("author", "Alice Smith"),
        ("capture", "Alice Smith"),
        ("revnumber", "1.0"),
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
    Ok(())
}

#[test]
fn header_metadata_same_author_retains_email_overrides() -> Result<(), Error> {
    let source = ":author: Alice Smith\n= T\nAlice Smith <before@example.org>\n:email: after@example.org\nv1.0, 2026-10-04";
    let parsed = parse(source, &Options::default())?;
    let header = parsed.document().header.as_ref().ok_or("missing header")?;
    let author = header.authors.first().ok_or("missing author")?;
    assert_eq!(author.email, Some("after@example.org"));
    Ok(())
}
