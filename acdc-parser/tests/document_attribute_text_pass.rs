use acdc_parser::{
    DocumentAttributeValue, Options,
    ast::{BlockKind, BlockRef, InlineKind, InlineRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

// Header values are not serialized in fixture JSON.
#[test]
fn document_attribute_text_pass_stores_definition_time_results() -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/subs_document_attribute_text_pass.adoc");
    let parsed = parse(source, &Options::default())?;
    for (name, expected) in [
        ("a", "Early *Bold*"),
        ("c", "&lt;x&gt; &amp; {name} *Bold*"),
        ("ac", "&lt;raw&gt; &amp;"),
        ("ca", "<raw> &"),
        ("normal-a", "&lt;ordinary&gt; &amp;"),
        ("normal-ac", "&amp;lt;ordinary&amp;gt; &amp;amp;"),
        ("normal-ca", "&lt;ordinary&gt; &amp;"),
        ("duplicate", "<raw> &"),
        ("none", "{name} &"),
        ("escaped", r"{name} {name} {name} \{name}"),
        ("nested", "pass:[Early]"),
        ("nested-c", "pass:c[Early]"),
        ("empty", ""),
        ("missing", "{not-set}"),
        ("brackets", r"Early a\]b"),
        ("verbatim", "&lt;x&gt; &amp; {name}"),
        ("long-verbatim", "&lt;x&gt; &amp; {name}"),
        ("entities", "&amp;#169; &amp;amp;"),
        ("unicode", "café &lt;α&gt; &amp;"),
        ("character", "<>&"),
    ] {
        assert_eq!(
            parsed
                .document_ref()
                .attributes()
                .get(name)
                .and_then(DocumentAttributeValue::text),
            Some(expected),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn document_attribute_text_pass_preserves_caller_policy() -> Result<(), Error> {
    let source = "= T\n:source: Document\n:value: pass:a[{source}]\n\nBody.\n";
    for (options, expected) in [
        (Options::default(), "Document"),
        (
            Options::with_attributes([("source", "<Caller> &")])?,
            "<Caller> &",
        ),
        (
            Options::builder()
                .with_attribute("source", "<Caller> &")
                .build()?,
            "<Caller> &",
        ),
        (
            Options::builder()
                .with_default_attribute("source", "<Default> &")
                .build()?,
            "Document",
        ),
        (
            Options::builder().with_attribute("source", false).build()?,
            "{source}",
        ),
        (
            Options::builder()
                .with_attribute("value", "pass:c[Caller]")
                .build()?,
            "pass:c[Caller]",
        ),
    ] {
        let parsed = parse(source, &options)?;
        assert_eq!(
            parsed
                .document_ref()
                .attributes()
                .get("value")
                .and_then(DocumentAttributeValue::text),
            Some(expected)
        );
    }
    Ok(())
}

#[test]
fn document_attribute_text_pass_keeps_complete_reference_spans() -> Result<(), Error> {
    for value in ["<é> &", "&#169; &amp;", "<tag>"] {
        for marker in ["", "*", "_", "`"] {
            let source = format!("= T\n:raw: pass:c[{value}]\n\nα {marker}{{raw}}{marker} ω.\n");
            let parsed = parse(&source, &Options::default())?;
            let [BlockKind::Paragraph(paragraph)] = &*parsed
                .document_ref()
                .blocks()
                .map(BlockRef::kind)
                .collect::<Vec<_>>()
            else {
                return Err("expected paragraph".into());
            };
            let mut nodes: Vec<_> = paragraph.content().inlines().collect();
            let mut count = 0;
            while let Some(node) = nodes.pop() {
                if let InlineKind::Bold(bold) = node.kind() {
                    nodes.extend(bold.content().inlines());
                } else if let InlineKind::Italic(italic) = node.kind() {
                    nodes.extend(italic.content().inlines());
                } else if let InlineKind::Monospace(mono) = node.kind() {
                    nodes.extend(mono.content().inlines());
                } else if matches!(
                    node.kind(),
                    InlineKind::Raw(_) | InlineKind::CharacterReference(_)
                ) {
                    count += 1;
                    assert_eq!(
                        &parsed.source()
                            [node.location().absolute_start()..=node.location().absolute_end()],
                        "{raw}"
                    );
                    assert_eq!(node.location().start().line(), 4);
                    assert_eq!(node.location().end().line(), 4);
                }
            }
            assert!(count > 0, "{source}");
        }
    }
    Ok(())
}

#[test]
fn document_attribute_text_pass_preprocessor_handles_line_endings() -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/document_attribute_text_pass_contexts.adoc");
    for newline in ["\n", "\r\n"] {
        let source = source.replace('\n', newline);
        let parsed = parse(&source, &Options::default())?;
        assert!(parsed.source().contains("T13 Definition-time condition."));
        assert!(parsed.source().contains("T14 Empty is set."));
        assert!(!parsed.source().contains("ifeval::"));
        assert!(!parsed.source().contains("ifdef::"));
    }
    Ok(())
}

#[test]
fn document_attribute_text_pass_resolves_include_targets() -> Result<(), Error> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/tests/document_attribute_text_pass_include.adoc");
    let parsed = acdc_parser::parse_file(
        path,
        &Options::builder()
            .with_safe_mode(acdc_parser::SafeMode::Unsafe)
            .build()?,
    )?;
    let [BlockKind::Paragraph(paragraph)] = &*parsed
        .document_ref()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected included paragraph".into());
    };
    assert!(
        matches!(&*paragraph.content().inlines().map(InlineRef::kind).collect::<Vec<_>>(), [InlineKind::Plain(text)] if text.text() == "Included.")
    );
    Ok(())
}
