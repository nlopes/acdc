use acdc_parser::{
    AttributeValue, CompatibilityMode, Options, SymbolKind,
    ast::{BlockKind, ContentRef, DocumentRef, InlineKind, StructuralRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[derive(Clone, Copy, Debug)]
enum Input {
    Document,
    Caller,
    Default,
}

fn paragraph(document: DocumentRef<'_>) -> Result<ContentRef<'_>, Error> {
    document
        .descendants()
        .find_map(|node| {
            if let StructuralRef::Block(block) = node
                && let BlockKind::Paragraph(paragraph) = block.kind()
            {
                Some(paragraph.content())
            } else {
                None
            }
        })
        .ok_or_else(|| "paragraph".into())
}

#[rstest::rstest]
#[case("->", SymbolKind::RightArrow)]
#[case("=>", SymbolKind::RightDoubleArrow)]
#[case("<-", SymbolKind::LeftArrow)]
#[case("<=", SymbolKind::LeftDoubleArrow)]
fn explicit_replacement_values_resolve_arrows_and_keep_reference_spans(
    #[case] value: &str,
    #[case] expected: SymbolKind,
    #[values(Input::Document, Input::Caller, Input::Default)] input: Input,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    for name in ["quot", "plus", "lt", "asterisk"] {
        let builder = Options::builder().with_compatibility_mode(mode);
        let reference = format!("{{{name}}}");
        let body = format!("α {reference} ω\n");
        let (source, options) = match input {
            Input::Document => (format!(":{name}: {value}\n\n{body}"), builder.build()?),
            Input::Caller => (body, builder.with_attribute(name, value).build()?),
            Input::Default => (body, builder.with_default_attribute(name, value).build()?),
        };
        let parsed = parse(&source, &options)?;
        assert!(
            parsed.warnings().is_empty(),
            "{name}: {:?}",
            parsed.warnings()
        );
        let content = paragraph(parsed.document())?;
        let before = serde_json::to_value(content)?;
        let nodes = content.inlines().collect::<Vec<_>>();
        let [_, inline, _] = nodes.as_slice() else {
            return Err(format!("one symbol between text: {name}: {nodes:?}").into());
        };
        let InlineKind::Symbol(symbol) = inline.kind() else {
            return Err(format!("resolved arrow: {name}: {inline:?}").into());
        };
        assert_eq!(symbol.kind(), expected, "{name}");
        let location = inline.location();
        assert_eq!(
            source.get(location.absolute_start()..=location.absolute_end()),
            Some(reference.as_str()),
            "{name}"
        );
        assert_eq!(location.start().column(), 3);
        assert_eq!(location.end().column(), 2 + u32::try_from(reference.len())?);
        #[cfg(feature = "asg")]
        for profile in [
            acdc_parser::asg::Profile::Upstream,
            acdc_parser::asg::Profile::Acdc,
        ] {
            acdc_parser::asg::inlines_with_profile(content, profile)?;
        }
        assert_eq!(serde_json::to_value(content)?, before);
    }
    Ok(())
}

#[rstest::rstest]
fn replacement_overrides_keep_header_snapshots_and_body_order(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":quot: ->\n\nBefore {quot}.\n\n:quot: <=\n\nAfter {quot}.\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(
        parsed
            .document()
            .attributes()
            .get("quot")
            .and_then(|value| value.text()),
        Some("->")
    );
    let symbols = parsed
        .document()
        .blocks()
        .filter_map(|block| {
            if let BlockKind::Paragraph(paragraph) = block.kind() {
                Some(paragraph.content())
            } else {
                None
            }
        })
        .flat_map(ContentRef::inlines)
        .filter_map(|inline| {
            if let InlineKind::Symbol(symbol) = inline.kind() {
                Some((symbol.kind(), inline.location()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(symbols.len(), 2);
    for ((actual, location), (expected, line)) in symbols.iter().zip([
        (SymbolKind::RightArrow, 3),
        (SymbolKind::LeftDoubleArrow, 7),
    ]) {
        assert_eq!(*actual, expected);
        assert_eq!(location.start().line(), line);
        assert_eq!(
            source.get(location.absolute_start()..=location.absolute_end()),
            Some("{quot}")
        );
    }
    Ok(())
}

#[rstest::rstest]
fn built_in_replacements_remain_literal_and_explicit_values_follow_quote_order(
    #[values(false, true)] nested: bool,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    for (subs, explicit_bold) in [("normal", false), ("attributes,quotes", true)] {
        let body = format!("[subs=\"{subs}\"]\n{{asterisk}}Bold{{asterisk}}\n");
        let source = if nested {
            format!("[cols=a,options=noheader]\n|===\n|\n{body}\n|===\n")
        } else {
            body
        };
        for explicit in [false, true] {
            let mut builder = Options::builder().with_compatibility_mode(mode);
            if explicit {
                builder = builder.with_attribute("asterisk", "*");
            }
            let parsed = parse(&source, &builder.build()?)?;
            assert_eq!(
                paragraph(parsed.document())?
                    .inlines()
                    .any(|inline| matches!(inline.kind(), InlineKind::Bold(_))),
                explicit && explicit_bold,
                "{subs}: {explicit}"
            );
        }
    }
    Ok(())
}

#[rstest::rstest]
fn replacement_overrides_do_not_repeat_macro_registration(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":quot: footnote:note[Only note.]\n\n{quot}\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [note] = &*parsed.document().footnotes().collect::<Vec<_>>() else {
        return Err("one registered note".into());
    };
    assert_eq!(note.id(), Some("note"));
    assert_eq!(
        paragraph(parsed.document())?
            .inlines()
            .filter(|inline| matches!(inline.kind(), InlineKind::Footnote(_)))
            .count(),
        1
    );
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn replacement_override_precedence_matches_ordinary_attributes(
    #[values(false, true)] unset_header: bool,
    #[values("default", "locked-set", "locked-unset", "soft-set", "soft-unset")] caller: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    for name in ["quot", "plus", "lt", "asterisk"] {
        let mut builder = Options::builder().with_compatibility_mode(mode);
        for key in ["ordinary", name] {
            builder = match caller {
                "default" => builder,
                "locked-set" => builder.with_attribute(key, "->"),
                "locked-unset" => builder.with_attribute(key, AttributeValue::None),
                "soft-set" => builder.with_default_attribute(key, "->"),
                "soft-unset" => builder.with_default_attribute(key, false),
                _ => return Err(format!("unknown input: {caller}").into()),
            };
        }
        let header = if unset_header {
            format!(":ordinary!:\n:{name}!:")
        } else {
            format!(":ordinary: =>\n:{name}: =>")
        };
        let source = format!(
            "{header}\n\n{{ordinary}} / {{{name}}}\n\n:ordinary: <=\n:{name}: <=\n\n{{ordinary}} / {{{name}}}\n\n:ordinary!:\n:{name}!:\n\n{{ordinary}} / {{{name}}}\n"
        );
        let parsed = parse(&source, &builder.build()?)?;
        let expected = match caller {
            "locked-set" => vec![SymbolKind::RightArrow; 6],
            "locked-unset" => vec![],
            _ if unset_header => vec![SymbolKind::LeftDoubleArrow; 2],
            _ => vec![
                SymbolKind::RightDoubleArrow,
                SymbolKind::RightDoubleArrow,
                SymbolKind::LeftDoubleArrow,
                SymbolKind::LeftDoubleArrow,
            ],
        };
        let actual = parsed
            .document()
            .blocks()
            .filter_map(|block| {
                if let BlockKind::Paragraph(paragraph) = block.kind() {
                    Some(paragraph.content())
                } else {
                    None
                }
            })
            .flat_map(ContentRef::inlines)
            .filter_map(|inline| {
                if let InlineKind::Symbol(symbol) = inline.kind() {
                    Some(symbol.kind())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{name}: {caller}: {unset_header}");
    }
    Ok(())
}
