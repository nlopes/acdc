use acdc_parser::{AttributeValue, Block, DocumentAttributeValue, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

fn plain_text<'a>(node: &InlineNode<'a>) -> Result<&'a str, Error> {
    if let InlineNode::PlainText(text) = node {
        Ok(text.content)
    } else if let InlineNode::RawText(text) = node {
        Ok(text.content)
    } else {
        Err(format!("expected text, got {node:?}").into())
    }
}

// Header values are omitted from fixture JSON; inspect their stored text too.
#[test]
fn document_attribute_pass_resolves_once_at_definition() -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/subs_document_attribute_pass.adoc");
    let parsed = parse(source, &Options::default())?;
    for (name, expected) in [
        ("plain", "Generated"),
        ("literal-ref", "{name}"),
        ("escaped-ref", r"\{name}"),
        ("empty", ""),
        ("partial", "prefix pass:[Embedded]"),
        ("escaped", r"\pass:[Escaped]"),
        ("brackets", r"a[b\]c]d"),
        ("alias", "Generated / {name}"),
        ("nested", "pass:[Nested]"),
        ("line", "first second"),
        ("unicode", "café α"),
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
fn document_attribute_pass_preserves_caller_policy() -> Result<(), Error> {
    let source = "= T\n:value: pass:[Header]\n\nStart.\n\n:value: pass:[Body]\n\n{value}\n";
    for (options, expected_header, expected_body) in [
        (Options::default(), "Header", "Body"),
        (
            Options::with_attributes([("value", AttributeValue::from("pass:[Caller]"))])?,
            "pass:[Caller]",
            "pass:[Caller]",
        ),
        (
            Options::builder()
                .with_attribute("value", "pass:[Caller]")
                .build()?,
            "pass:[Caller]",
            "pass:[Caller]",
        ),
        (
            Options::builder()
                .with_default_attribute("value", "pass:[Caller]")
                .build()?,
            "Header",
            "Body",
        ),
    ] {
        let parsed = parse(source, &options)?;
        assert_eq!(
            parsed
                .document()
                .attributes
                .get("value")
                .and_then(DocumentAttributeValue::text),
            Some(expected_header)
        );
        let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.last() else {
            return Err("expected body paragraph".into());
        };
        assert_eq!(
            paragraph
                .content
                .iter()
                .map(plain_text)
                .collect::<Result<String, Error>>()?,
            expected_body
        );
    }
    let options = Options::builder().with_attribute("value", false).build()?;
    assert!(
        !parse(source, &options)?
            .document()
            .attributes
            .contains_key("value")
    );
    Ok(())
}

#[test]
fn document_attribute_pass_is_available_to_conditionals() -> Result<(), Error> {
    for newline in ["\n", "\r\n"] {
        let source = "= T\n:name: Early\n:value: pass:[{name}]\n:empty: pass:[]\n:flag: pass:[yes]\n\nifeval::[\"{flag}\" == \"yes\"]\nValue: {value}\nendif::[]\n\nifdef::empty[]\nEmpty is set.\nendif::[]\n".replace('\n', newline);
        let parsed = parse(&source, &Options::default())?;
        assert_eq!(
            parsed
                .document()
                .attributes
                .get("value")
                .and_then(DocumentAttributeValue::text),
            Some("{name}")
        );
        assert!(parsed.document().attributes.contains_key("empty"));
        let text: String = parsed
            .document()
            .blocks
            .iter()
            .filter_map(|block| {
                if let Block::Paragraph(p) = block {
                    Some(
                        p.content
                            .iter()
                            .map(plain_text)
                            .collect::<Result<String, Error>>(),
                    )
                } else {
                    None
                }
            })
            .collect::<Result<String, Error>>()?;
        assert!(text.contains("Value: {name}"), "{text}");
        assert!(text.contains("Empty is set."), "{text}");
    }
    Ok(())
}

#[test]
fn document_attribute_pass_is_available_to_include_targets() -> Result<(), Error> {
    let main = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/tests/document_attribute_pass_include.adoc");
    let parsed = acdc_parser::parse_file(
        &main,
        &Options::builder()
            .with_safe_mode(acdc_parser::SafeMode::Unsafe)
            .build()?,
    )?;
    let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
        return Err("expected included paragraph".into());
    };
    assert_eq!(
        paragraph
            .content
            .iter()
            .map(plain_text)
            .collect::<Result<String, Error>>()?,
        "Included."
    );
    Ok(())
}

#[test]
fn document_attribute_pass_raw_fragments_keep_reference_locations() -> Result<(), Error> {
    for value in ["&", "<é>", "é & α", "&#169;", "&amp;"] {
        for marker in ["", "*", "_", "`"] {
            let source = format!("= T\n:raw: pass:[{value}]\n\nα {marker}{{raw}}{marker} ω.\n");
            let parsed = parse(&source, &Options::default())?;
            let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                return Err("expected paragraph".into());
            };
            let mut nodes: Vec<_> = paragraph.content.iter().collect();
            let mut raw_count = 0;
            while let Some(node) = nodes.pop() {
                if let InlineNode::BoldText(b) = node {
                    nodes.extend(&b.content);
                } else if let InlineNode::ItalicText(i) = node {
                    nodes.extend(&i.content);
                } else if let InlineNode::MonospaceText(m) = node {
                    nodes.extend(&m.content);
                }
                if let InlineNode::RawText(raw) = node {
                    raw_count += 1;
                    let location = &raw.location;
                    assert_eq!(
                        &parsed.source()[location.absolute_start..=location.absolute_end],
                        "{raw}"
                    );
                    assert_eq!(location.start.line, 4);
                    assert_eq!(location.end.line, 4);
                }
            }
            assert!(raw_count > 0, "{source}");
        }
    }
    Ok(())
}

#[test]
fn document_attribute_pass_explicit_quotes_do_not_expand_attributes() -> Result<(), Error> {
    let parsed = parse(
        "= T\n:name: Early\n:value: pass:q[*{name}*]\n\nBody.\n",
        &Options::default(),
    )?;
    assert_eq!(
        parsed
            .document()
            .attributes
            .get("value")
            .and_then(DocumentAttributeValue::text),
        Some("*{name}*")
    );
    Ok(())
}
