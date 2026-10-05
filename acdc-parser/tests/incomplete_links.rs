use acdc_parser::{InlineMacro, InlineNode, Options, parse_inline};

type Error = Box<dyn std::error::Error>;

#[test]
fn incomplete_links_do_not_duplicate_nested_notes() -> Result<(), Error> {
    let parsed = acdc_parser::parse(
        include_str!("../fixtures/tests/incomplete_links.adoc"),
        &Options::default(),
    )?;
    let [note] = parsed.document().footnotes.as_slice() else {
        return Err("expected one note from the incomplete link's remaining text".into());
    };
    let [InlineNode::PlainText(body)] = note.content.as_slice() else {
        return Err("expected plain note body".into());
    };
    assert_eq!(body.content, "Only note.");
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn incomplete_links_keep_targets_literal_and_source_spans(
    #[values(
        "https://example.org",
        "http://example.org",
        "ftp://example.org",
        "irc://example.org",
        "mailto:user@example.org",
        "user@example.org",
        "https://user@example.org",
        "\\https://example.org",
        "\\\\https://example.org",
        "manual.html",
        "école.html"
    )]
    target: &str,
    #[values("", "[Unfinished", "[Escaped\\]")] suffix: &str,
) -> Result<(), Error> {
    let source = format!("α link:{target}{suffix}");
    let parsed = parse_inline(&source, &Options::default())?;
    let mut text = String::new();
    for node in parsed.inlines() {
        let InlineNode::PlainText(plain) = node else {
            return Err(format!("unexpected node for {source}: {node:?}").into());
        };
        text.push_str(plain.content);
        let location = &plain.location;
        assert_eq!(
            &source[location.absolute_start..=location.absolute_end],
            plain.content
        );
        assert_eq!(
            location.start.column as usize,
            source[..location.absolute_start].chars().count() + 1
        );
    }
    assert_eq!(text, source);
    Ok(())
}

#[test]
fn incomplete_links_preserve_complete_nested_macros() -> Result<(), Error> {
    let source = "link:https://unfinished.example[Before https://valid.example[Valid]";
    let parsed = parse_inline(source, &Options::default())?;
    let links: Vec<_> = parsed
        .inlines()
        .iter()
        .filter_map(|node| {
            if let InlineNode::Macro(InlineMacro::Url(link)) = node {
                Some(link)
            } else {
                None
            }
        })
        .collect();
    let [link] = links.as_slice() else {
        return Err("expected the complete inner link".into());
    };
    assert_eq!(link.target.to_string(), "https://valid.example");
    assert_eq!(
        &source[link.location.absolute_start..=link.location.absolute_end],
        "https://valid.example[Valid]"
    );
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn incomplete_links_cannot_be_completed_by_late_attributes() -> Result<(), Error> {
    use acdc_parser::{Block, parse};

    for label in ["{open}Label]", "[Label{close}", "{open}Label{close}"] {
        for (subs, linked) in [("macros,attributes", false), ("attributes,macros", true)] {
            let source = format!(
                ":open: [\n:close: ]\n\n[subs=\"{subs}\"]\nlink:https://example.org{label}\n"
            );
            let parsed = parse(&source, &Options::default())?;
            let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                return Err("expected paragraph".into());
            };
            assert_eq!(
                paragraph
                    .content
                    .iter()
                    .any(|node| matches!(node, InlineNode::Macro(_))),
                linked,
                "{source}"
            );
        }
    }
    Ok(())
}
