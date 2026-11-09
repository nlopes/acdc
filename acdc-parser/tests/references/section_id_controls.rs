use acdc_parser::{
    CompatibilityMode, Options, SafeMode,
    ast::{BlockKind, InlineKind, TocEntryRef},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

const MODES: [CompatibilityMode; 2] = [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft];

#[test]
fn section_identity_uses_the_settings_at_each_heading() -> Result<(), Error> {
    for mode in MODES {
        let parsed = parse(
            include_str!("../../fixtures/tests/section_id_controls.adoc"),
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let document = parsed.document();
        let ids: Vec<_> = document.toc_entries().map(TocEntryRef::id).collect();
        assert_eq!(
            ids,
            [
                "custom_same-words-2",
                "custom_same-words-3",
                "emptywords",
                "emptywords2",
                "FINAL.tail.words",
                "FINAL.tail.words.2",
                "authored_id",
                "",
                "2",
            ],
        );
        for id in ids {
            assert!(document.references().contains(id), "{id}");
        }
        let paragraph = document
            .blocks()
            .filter_map(|block| {
                if let BlockKind::Paragraph(paragraph) = block.kind() {
                    Some(paragraph)
                } else {
                    None
                }
            })
            .find(|paragraph| {
                paragraph
                    .content()
                    .inlines()
                    .any(|inline| matches!(inline.kind(), InlineKind::CrossReference(_)))
            })
            .ok_or("missing forward references")?;
        let targets: Vec<_> = paragraph
            .content()
            .inlines()
            .filter_map(|inline| {
                if let InlineKind::CrossReference(xref) = inline.kind() {
                    Some(xref.target())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            targets,
            ["custom_same-words-2", "emptywords", "FINAL.tail.words"]
        );
        assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    }
    Ok(())
}

#[test]
fn separators_preserve_literal_underscores_and_prefix_case() -> Result<(), Error> {
    for mode in MODES {
        for (prefix, separator, expected) in [
            ("PREFIX_", "-", "PREFIX_über_word-a-b"),
            ("", ".", "über_word.a.b"),
            ("", "", "über_worda-b"),
            ("UP-", "--", "UP-über_word-a-b"),
            ("", "é", "über_wordéaéb"),
        ] {
            let source = format!(
                "= T\n:idprefix: {prefix}\n:idseparator: {separator}\n\n== Über_word A-B\n"
            );
            let parsed = parse(
                &source,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            assert_eq!(
                parsed
                    .document()
                    .toc_entries()
                    .next()
                    .ok_or("missing heading")?
                    .id(),
                expected
            );
        }
    }
    Ok(())
}

#[test]
fn collision_suffixes_follow_the_current_separator() -> Result<(), Error> {
    for mode in MODES {
        let parsed = parse(
            "= T\n:idprefix:\n:idseparator: -\n\n== Same\n\n== Same\n\n:idseparator: .\n\n== Same\n\n== Same\n\n:idseparator:\n\n== Same\n\n:idprefix!:\n:idseparator!:\n\n== Same\n",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        assert_eq!(
            parsed
                .document()
                .toc_entries()
                .map(TocEntryRef::id)
                .collect::<Vec<_>>(),
            ["same", "same-2", "same.2", "same.3", "same2", "_same"]
        );
    }
    Ok(())
}

#[test]
fn nested_headings_keep_their_own_settings() -> Result<(), Error> {
    for mode in MODES {
        let parsed = parse(
            "= T\n:idprefix: parent_\n:idseparator: -\n\n== Parent Words\n\n:idprefix: child.\n:idseparator: .\n\n=== Child Words\n\n[cols=a]\n|===\n|== Cell Words\n\ncell\n|===\n\n== Tail Words\n",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let document = parsed.document();
        assert_eq!(
            document
                .toc_entries()
                .map(TocEntryRef::id)
                .collect::<Vec<_>>(),
            [
                "parent_parent-words",
                "child.child.words",
                "child.tail.words"
            ]
        );
        assert!(document.references().contains("child.cell.words"));
    }
    Ok(())
}

#[test]
fn included_and_table_headings_use_their_declaration_context() -> Result<(), Error> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/tests/section_id_contexts.adoc");
    for mode in MODES {
        let parsed = parse_file(
            &path,
            &Options::builder()
                .with_safe_mode(SafeMode::Unsafe)
                .with_compatibility_mode(mode)
                .build()?,
        )?;
        let document = parsed.document();
        assert_eq!(
            document
                .toc_entries()
                .map(TocEntryRef::id)
                .collect::<Vec<_>>(),
            [
                "Before_parent-words",
                "Nested.included.words",
                "Nested.tail.words"
            ]
        );
        for id in [
            "Before_parent-words",
            "Nested.included.words",
            "Nested.cell.words",
            "Nested.tail.words",
        ] {
            assert!(document.references().contains(id), "{id}");
        }
        assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    }
    Ok(())
}

#[test]
fn custom_id_settings_apply_to_setext_headings() -> Result<(), Error> {
    let parsed = parse(
        "= T\n:idprefix: SET_\n:idseparator: -\n\nSame Words\n----------\n\n== Same Words\n",
        &Options::default(),
    )?;
    assert_eq!(
        parsed
            .document()
            .toc_entries()
            .map(TocEntryRef::id)
            .collect::<Vec<_>>(),
        ["SET_same-words", "SET_same-words-2"]
    );
    Ok(())
}
