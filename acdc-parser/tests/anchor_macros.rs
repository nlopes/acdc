use acdc_parser::{InlineNode, Options, WarningKind, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn anchor_macros_register_labels_and_exact_source_spans() -> Result<(), Error> {
    // The reference catalog and diagnostics are omitted from JSON snapshots.
    let source = include_str!("../fixtures/tests/anchor_macro.adoc");
    let parsed = parse(source, &Options::default())?;
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    for (id, syntax) in [
        ("target", "anchor:target[Target *label*]"),
        ("empty", "anchor:empty[]"),
        ("expanded", "anchor:{target}[{label}]"),
        ("bullet", "anchor:bullet[Bullet label]"),
        ("term", "anchor:term[Term label]"),
        ("cell", "anchor:cell[Cell label]"),
        ("note", "anchor:note[Note label]"),
        ("xref-label", "anchor:xref-label[Inner label]"),
        ("url-label", "anchor:url-label[URL label]"),
        ("first", "anchor:first[First]"),
        ("second", "anchor:second[Second]"),
    ] {
        let reference = parsed.document().references.get(id).ok_or(id)?;
        assert_eq!(
            source.get(reference.location.absolute_start..=reference.location.absolute_end),
            Some(syntax),
            "{id}"
        );
        assert_eq!(reference.xreflabel.is_some(), id != "empty", "{id}");
    }
    assert_eq!(parsed.document().footnotes.len(), 1);
    Ok(())
}

#[test]
fn anchor_macros_warn_on_duplicates_and_keep_first_label() -> Result<(), Error> {
    let source =
        "anchor:same[First]One.\n\nanchor:same[Second]Two.\n\n[[same,Third]]Three.\n\n<<same>>.\n";
    let parsed = parse(source, &Options::default())?;
    assert_eq!(parsed.warnings().len(), 2);
    for (warning, syntax) in parsed
        .warnings()
        .iter()
        .zip(["anchor:same[Second]", "[[same,Third]]"])
    {
        let WarningKind::DuplicateId { id, first } = &warning.kind else {
            return Err(format!("unexpected warning: {warning}").into());
        };
        assert_eq!(id, "same");
        assert_eq!(
            source.get(first.location.absolute_start..=first.location.absolute_end),
            Some("anchor:same[First]")
        );
        let duplicate = &warning
            .source_location()
            .ok_or("missing location")?
            .location;
        assert_eq!(
            source.get(duplicate.absolute_start..=duplicate.absolute_end),
            Some(syntax)
        );
    }
    let label = parsed
        .document()
        .references
        .get("same")
        .and_then(|reference| reference.xreflabel.as_deref())
        .ok_or("missing label")?;
    assert!(matches!(label, [InlineNode::PlainText(text)] if text.content == "First"));
    Ok(())
}

#[test]
fn anchor_macros_do_not_register_escaped_invalid_or_literal_ids() -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/anchor_macro_boundaries.adoc");
    let parsed = parse(source, &Options::default())?;
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    let references = &parsed.document().references;
    for id in [
        "café",
        "東京",
        "cafe\u{301}",
        "a\u{203f}b",
        ":colon",
        "_under",
        "a-b.c:d",
        "joined",
        "comma",
        "quoted",
        "equals",
        "bracket",
        "open",
        "topic🚀",
        "a·b",
        "a\u{200d}b",
    ] {
        assert!(references.contains_key(id), "missing {id}");
    }
    for id in [
        "escaped",
        "twice",
        "1bad",
        "bad/id",
        "bad",
        "bad@id",
        "bad%id",
        "bad,id",
        "bad#id",
        "unfinished",
        "wrapped",
        "single",
        "double",
        "raw",
        "code",
        "indented",
        "🚀bad",
        "bad\u{a0}id",
    ] {
        assert!(!references.contains_key(id), "unexpected {id}");
    }
    Ok(())
}

#[test]
fn anchor_macros_preserve_unicode_ids_without_normalizing() -> Result<(), Error> {
    for id in [
        "café",
        "cafe\u{301}",
        "東京",
        "\u{10400}name",
        "a\u{488}b",
        "a\u{903}b",
        "a\u{fe0f}b",
        "a\u{203f}b",
        "a\u{ff3f}b",
        "a١b",
        "topic🚀",
        "topic©",
        "a–b",
        "a·b",
        "a\u{200d}b",
        "a\u{ad}b",
    ] {
        let source = format!("anchor:{id}[Label]Target.\n\n<<{id}>>.\n");
        let parsed = parse(&source, &Options::default())?;
        let references = &parsed.document().references;
        assert_eq!(references.len(), 1, "{id:?}");
        assert!(references.contains_key(id), "missing {id:?}");
        assert!(
            parsed.warnings().is_empty(),
            "{id:?}: {:?}",
            parsed.warnings()
        );
    }
    Ok(())
}

#[test]
fn anchor_macros_reject_whitespace_controls_and_invalid_starts() -> Result<(), Error> {
    for id in [
        "a\0b",
        "a\u{1f}b",
        "a\u{7f}b",
        "a\u{85}b",
        "a\u{9f}b",
        "a b",
        "a\tb",
        "a\nb",
        "a\rb",
        "a\u{a0}b",
        "a\u{1680}b",
        "a\u{2003}b",
        "a\u{2028}b",
        "a\u{2029}b",
        "a\u{202f}b",
        "a\u{205f}b",
        "a\u{3000}b",
        "\u{301}bad",
        "\u{203f}bad",
        "🚀bad",
        "١bad",
        "1bad",
        "-bad",
        ".bad",
        "a/b",
        "a@b",
    ] {
        let source = format!("anchor:{id}[Label]Target.\n");
        let parsed = parse(&source, &Options::default())?;
        assert!(parsed.document().references.is_empty(), "unexpected {id:?}");
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn anchor_macros_follow_macro_and_attribute_substitution_order() -> Result<(), Error> {
    for (subs, recognized) in [
        ("attributes,macros", true),
        ("macros,attributes", false),
        ("attributes", false),
        ("none", false),
    ] {
        for syntax in [
            "anchor:{target}[Label]",
            "{macro}[Label]",
            "an{empty}chor:dynamic[Label]",
        ] {
            let source = format!(
                ":target: dynamic\n:macro: anchor:dynamic\n:empty:\n\n[subs=\"{subs}\"]\n{syntax}Text.\n"
            );
            let parsed = parse(&source, &Options::default())?;
            assert_eq!(
                parsed.document().references.contains_key("dynamic"),
                recognized,
                "{source}"
            );
        }
    }
    Ok(())
}
