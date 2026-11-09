use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

// Fixture JSON does not include footnote catalogs or reference diagnostics.
#[test]
fn outer_macro_escapes_do_not_register_literal_macros() -> Result<(), Error> {
    let parsed = parse(
        include_str!("../../fixtures/tests/outer_macro_escapes.adoc"),
        &Options::default(),
    )?;
    assert_eq!(parsed.document().footnotes().count(), 1);
    assert!(!parsed.document().references().contains("unused"));
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[test]
fn outer_macro_escapes_preserve_backslash_runs_and_unicode_source_spans() -> Result<(), Error> {
    for count in 1..=4 {
        for literal in [
            "indexterm2:[é café]",
            "((é café))",
            "xref:target[é café]",
            "footnote:[é café]",
            "mailto:user@example.org[é café]",
            "link:manual.html[é café]",
            "anchor:unused[é café]",
            "https://example.org[é café]",
        ] {
            let source = format!("α {}{literal} ω.\n", "\\".repeat(count));
            let parsed = parse(&source, &Options::default())?;
            let [BlockKind::Paragraph(paragraph)] = &*parsed
                .document()
                .blocks()
                .map(BlockRef::kind)
                .collect::<Vec<_>>()
            else {
                return Err("expected one paragraph".into());
            };
            let mut text = String::new();
            for node in paragraph.content().inlines() {
                let InlineKind::Plain(plain) = node.kind() else {
                    return Err(format!("escaped macro became active: {node:?}").into());
                };
                text.push_str(plain.text());
                if plain.text().ends_with(literal) {
                    let source_escapes = if literal.starts_with("https:") {
                        count
                    } else {
                        1
                    };
                    assert_eq!(
                        &source
                            [plain.location().absolute_start()..=plain.location().absolute_end()],
                        format!("{}{literal}", "\\".repeat(source_escapes))
                    );
                }
                for (offset, position) in [
                    (plain.location().absolute_start(), plain.location().start()),
                    (plain.location().absolute_end(), plain.location().end()),
                ] {
                    assert!(source.is_char_boundary(offset), "{source}");
                    assert_eq!(position.line(), 1);
                    assert_eq!(
                        position.column() as usize,
                        source[..offset].chars().count() + 1
                    );
                }
            }
            let consumed = usize::from(count == 1 || !literal.starts_with("https:"));
            assert_eq!(
                text,
                format!("α {}{literal} ω.", "\\".repeat(count - consumed))
            );
            assert!(parsed.document().footnotes().next().is_none());
            assert!(parsed.document().references().is_empty());
            assert_eq!(parsed.warnings(), []);
        }
    }
    Ok(())
}

#[test]
fn outer_macro_escapes_cannot_be_introduced_after_macro_substitution() -> Result<(), Error> {
    for (subs, expected_macros) in [("macros,attributes", 2), ("attributes,macros", 0)] {
        let source = format!(
            ":escape: \\\\\n\n[subs=\"{subs}\"]\n{{escape}}indexterm2:[Index] and {{escape}}footnote:[Note] and {{escape}}https://example.org[URL].\n"
        );
        let parsed = parse(&source, &Options::default())?;
        let Some(BlockKind::Paragraph(paragraph)) =
            parsed.document().blocks().last().map(BlockRef::kind)
        else {
            return Err("expected paragraph".into());
        };
        let macros = paragraph
            .content()
            .inlines()
            .filter(|node| {
                matches!(
                    node.kind(),
                    InlineKind::IndexTerm(_) | InlineKind::Footnote(_)
                )
            })
            .count();
        assert_eq!(macros, expected_macros, "{subs}");
        assert!(!paragraph.content().inlines().any(|node| matches!(
            node.kind(),
            InlineKind::UrlMacro(_) | InlineKind::Autolink(_)
        )));
        assert_eq!(
            parsed.document().footnotes().count(),
            usize::from(expected_macros != 0)
        );
    }
    Ok(())
}
