use acdc_parser::{
    CompatibilityMode, Options, SymbolKind,
    ast::{AttributeValueRef, ContentRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn text(content: ContentRef<'_>) -> Result<String, Error> {
    let mut result = String::new();
    for node in content.inlines() {
        let kind = node.kind();
        let text = if let InlineKind::Plain(text) = kind {
            text.text()
        } else if let InlineKind::Symbol(symbol) = kind {
            symbol.text()
        } else if let InlineKind::Raw(raw) = kind {
            raw.text()
        } else {
            return Err(format!("expected resolved or literal text: {kind:?}").into());
        };
        result.push_str(text);
    }
    Ok(result)
}

#[rstest::rstest]
fn content_attribute_edges_are_independent_of_body_substitutions(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("title", "attribution", "citetitle", "caption")] name: &str,
    #[values("'", "\"")] quote: &str,
) -> Result<(), Error> {
    let value = "-- π (C) -> --";
    let source =
        format!("Earlier.\n\n[quote,{name}={quote}{value}{quote},subs=none]\n____\nBody.\n____");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let block = parsed.document_ref().blocks().last().ok_or("block")?;
    let Some(AttributeValueRef::Content(content)) =
        block.attributes().and_then(|attrs| attrs.get(name))
    else {
        return Err(format!("missing {name}").into());
    };
    let formatted = quote == "'" || (name == "title" && mode == CompatibilityMode::Asciidoctor);
    assert_eq!(
        text(content)?,
        if formatted {
            "\u{2009}—\u{2009}π © →\u{2009}—\u{2009}"
        } else {
            value
        }
    );
    let last = content.inlines().last().ok_or("last node")?;
    let last_offset = source.find(value).ok_or("authored value")? + value.len() - 1;
    assert_eq!(last.location().absolute_end(), last_offset);
    if formatted {
        let InlineKind::Symbol(symbol) = last.kind() else {
            return Err("expected resolved trailing dash".into());
        };
        assert_eq!(symbol.kind(), SymbolKind::SpacedEmDash);
    }
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
fn isolated_attribute_edges_do_not_leak_into_nested_link_labels(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "[quote,attribution='link:https://example.org[--] --']\n____\nBody.\n____";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let block = parsed.document_ref().blocks().next().ok_or("block")?;
    let Some(AttributeValueRef::Content(content)) = block
        .attributes()
        .and_then(|attrs| attrs.get("attribution"))
    else {
        return Err("attribution".into());
    };
    let first = content.inlines().next().ok_or("first")?;
    let InlineKind::Link(link) = first.kind() else {
        return Err("link".into());
    };
    assert_eq!(text(link.text())?, "--");
    let last = content.inlines().last().ok_or("last")?;
    assert!(
        matches!(last.kind(), InlineKind::Symbol(symbol) if symbol.kind() == SymbolKind::SpacedEmDash)
    );
    Ok(())
}
