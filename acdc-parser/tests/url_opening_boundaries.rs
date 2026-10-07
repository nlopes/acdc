#[cfg(feature = "pre-spec-subs")]
use acdc_parser::{Block, DelimitedBlockType, parse};
use acdc_parser::{InlineMacro, InlineNode, Location, Options, parse_inline};

type Error = Box<dyn std::error::Error>;

fn collect_links(nodes: &[InlineNode<'_>], links: &mut Vec<(String, Location)>) {
    for node in nodes {
        if let InlineNode::Macro(InlineMacro::Autolink(link)) = node {
            links.push((link.url.to_string(), link.location.clone()));
        } else if let InlineNode::Macro(InlineMacro::Url(link)) = node {
            links.push((link.target.to_string(), link.location.clone()));
        } else if let InlineNode::Macro(InlineMacro::Link(link)) = node {
            links.push((link.target.to_string(), link.location.clone()));
        } else if let InlineNode::BoldText(bold) = node {
            collect_links(&bold.content, links);
        } else if let InlineNode::ItalicText(italic) = node {
            collect_links(&italic.content, links);
        } else if let InlineNode::MonospaceText(code) = node {
            collect_links(&code.content, links);
        } else if let InlineNode::CurvedQuotationText(quote) = node {
            collect_links(&quote.content, links);
        } else if let InlineNode::CurvedApostropheText(quote) = node {
            collect_links(&quote.content, links);
        }
    }
}

#[test]
fn automatic_urls_require_an_opening_boundary() -> Result<(), Error> {
    for prefix in [
        "Link:", "prefix", "label:", ",", "=", "_prefix", "café", "e\u{301}",
    ] {
        for suffix in ["", "[Site]"] {
            let source = format!("{prefix}https://example.org/path{suffix}");
            let parsed = parse_inline(&source, &Options::default())?;
            let mut links = Vec::new();
            collect_links(parsed.inlines(), &mut links);
            assert!(links.is_empty(), "unexpected link in {source:?}: {links:?}");
            let text: String = parsed
                .inlines()
                .iter()
                .filter_map(|node| {
                    if let InlineNode::PlainText(plain) = node {
                        Some(plain.content)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(text, source);
        }
    }
    Ok(())
}

#[test]
fn quoted_urls_need_labels_to_become_links() -> Result<(), Error> {
    for quote in ['\'', '"'] {
        for (suffix, count) in [("", 0), ("[Site]", 1), ("[]", 1)] {
            let source = format!("{quote}https://example.org/path{suffix}{quote}");
            let parsed = parse_inline(&source, &Options::default())?;
            let mut links = Vec::new();
            collect_links(parsed.inlines(), &mut links);
            assert_eq!(links.len(), count, "{source:?}");
        }
    }
    Ok(())
}

#[test]
fn valid_url_boundaries_preserve_targets_and_source_spans() -> Result<(), Error> {
    for prefix in ["", " ", "\t", "\n", "(", ")", "[", "]", ";", ">", "\u{a0}"] {
        for suffix in ["", "[Site]"] {
            let uri = format!("https://example.org/path{suffix}");
            let source = format!("{prefix}{uri}");
            let parsed = parse_inline(&source, &Options::default())?;
            let mut links = Vec::new();
            collect_links(parsed.inlines(), &mut links);
            let [(target, location)] = links.as_slice() else {
                return Err(format!("expected one URL: {source:?}: {links:?}").into());
            };
            assert_eq!(target, "https://example.org/path");
            assert_eq!(
                &source[location.absolute_start..=location.absolute_end],
                uri
            );
            assert_eq!(location.absolute_start, prefix.len());
        }
    }
    Ok(())
}

#[test]
fn escaped_urls_require_the_same_opening_boundary() -> Result<(), Error> {
    for (prefix, boundary) in [
        ("", true),
        ("\"", true),
        ("'", true),
        ("(", true),
        ("A", false),
        ("é:", false),
    ] {
        for count in 1..=3 {
            for suffix in ["", "[Site]"] {
                let source = format!(
                    "{prefix}{}https://example.org/path{suffix}",
                    "\\".repeat(count)
                );
                let parsed = parse_inline(&source, &Options::default())?;
                let mut text = String::new();
                for node in parsed.inlines() {
                    let InlineNode::PlainText(plain) = node else {
                        return Err(
                            format!("escaped URL became active: {source:?}: {node:?}").into()
                        );
                    };
                    text.push_str(plain.content);
                    let location = &plain.location;
                    assert!(source.is_char_boundary(location.absolute_start));
                    assert!(source.is_char_boundary(location.absolute_end));
                    assert_eq!(
                        location.start.column as usize,
                        source[..location.absolute_start].chars().count() + 1
                    );
                    assert_eq!(
                        location.end.column as usize,
                        source[..location.absolute_end].chars().count() + 1
                    );
                }
                let removed = usize::from(boundary && count == 1);
                assert_eq!(
                    text,
                    format!(
                        "{prefix}{}https://example.org/path{suffix}",
                        "\\".repeat(count - removed)
                    ),
                    "{source:?}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn completed_formatting_can_supply_a_url_boundary() -> Result<(), Error> {
    for (before, after, expected) in [
        ("**Prefix**", "", 1),
        ("__Prefix__", "", 1),
        ("``Prefix``", "", 1),
        ("*", "*", 1),
        ("_", "_", 1),
        ("**outer _", "_**", 1),
        ("\"`", "`\"", 1),
        ("'`", "`'", 1),
        ("*Prefix*", "", 0),
        ("**Prefix", "", 0),
        ("**Prefix**word", "", 0),
        ("**Prefix**unmatched*", "", 0),
        ("pass:[X]", "", 0),
        ("pass:[ ]", "", 0),
        ("pass:[*]", "", 0),
    ] {
        for suffix in ["", "[Site]"] {
            let uri = format!("https://example.org/path{suffix}");
            let source = format!("{before}{uri}{after}");
            let parsed = parse_inline(&source, &Options::default())?;
            let mut links = Vec::new();
            collect_links(parsed.inlines(), &mut links);
            assert_eq!(links.len(), expected, "{source:?}: {:?}", parsed.inlines());
            for (target, location) in &links {
                assert_eq!(target, "https://example.org/path");
                assert_eq!(
                    &source[location.absolute_start..=location.absolute_end],
                    uri
                );
            }
        }
    }
    Ok(())
}

#[test]
fn explicit_links_and_neighboring_url_macros_keep_their_boundaries() -> Result<(), Error> {
    for source in [
        "Label:link:https://example.org/one[Site]",
        "link:https://example.org/one[Site]",
        "https://example.org/one[One]https://example.org/two[Two]",
        "xref:target[https://example.org/one[Site]]",
    ] {
        let parsed = parse_inline(source, &Options::default())?;
        let mut links = Vec::new();
        let nodes = if let [InlineNode::Macro(InlineMacro::CrossReference(xref))] = parsed.inlines()
        {
            xref.text.as_slice()
        } else {
            parsed.inlines()
        };
        collect_links(nodes, &mut links);
        let expected = if source.contains("/two") { 2 } else { 1 };
        assert_eq!(links.len(), expected, "{source:?}");
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn url_opening_boundaries_use_the_macro_stage() -> Result<(), Error> {
    for (subs, body, expected) in [
        (
            "attributes,macros",
            "{space}https://example.org/path[Site]",
            1,
        ),
        (
            "macros,attributes",
            "{space}https://example.org/path[Site]",
            0,
        ),
        (
            "attributes,macros",
            "{empty}https://example.org/path[Site]",
            1,
        ),
        (
            "macros,attributes",
            "{empty}https://example.org/path[Site]",
            0,
        ),
        (
            "attributes,macros",
            "{prefix}https://example.org/path[Site]",
            0,
        ),
        (
            "macros,attributes",
            "{prefix}https://example.org/path[Site]",
            0,
        ),
        (
            "quotes,macros",
            "**Prefix**https://example.org/path[Site]",
            1,
        ),
        (
            "macros,quotes",
            "**Prefix**https://example.org/path[Site]",
            0,
        ),
        ("none", "https://example.org/path[Site]", 0),
        ("-macros", "https://example.org/path", 0),
    ] {
        let source =
            format!(":space: {{sp}}\n:empty:\n:prefix: Label:\n\n[subs=\"{subs}\"]\n{body}");
        let parsed = parse(&source, &Options::default())?;
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected a paragraph".into());
        };
        let mut links = Vec::new();
        collect_links(&paragraph.content, &mut links);
        assert_eq!(links.len(), expected, "{source:?}");
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn formatted_attribute_profiles_preserve_url_boundaries() -> Result<(), Error> {
    for (subs, prefix, expected) in [
        ("normal", "{bold}", 1),
        ("attributes,macros", "{bold}", 1),
        ("macros,attributes", "{bold}", 0),
        ("normal", "**Bold**{blank}", 1),
        ("normal", "{bold}{blank}", 1),
        ("normal", "{bold}unmatched*", 0),
        ("normal", "{raw}", 0),
        ("normal", "{tail}", 1),
        ("normal", "{punct}", 1),
        ("normal", "{space}", 1),
        ("normal", "Text {blank}", 1),
        ("normal", "Text;{blank}", 1),
        ("normal", "Text{blank}", 0),
        ("normal", "{alias}", 0),
        ("normal", "{alias}{blank}", 0),
        ("macros,attributes", "Text {blank}", 0),
    ] {
        for suffix in ["", "[Site]"] {
            let source = format!(
                ":bold: pass:q[*Bold*]\n:blank: pass:q[]\n:raw: pass:[X]\n:tail: pass:q[*Bold* ]\n:punct: pass:q[*Bold*;]\n:space: pass:[ ]\n:empty-replacement: pass:r[]\n:alias: {{bold}}X{{empty-replacement}}\n\n[subs=\"{subs}\"]\n{prefix}https://example.org/path{suffix}"
            );
            let parsed = parse(&source, &Options::default())?;
            let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                return Err("expected a paragraph".into());
            };
            let mut links = Vec::new();
            collect_links(&paragraph.content, &mut links);
            assert_eq!(links.len(), expected, "{source:?}: {:?}", paragraph.content);
        }
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn enabled_code_links_use_the_same_boundaries() -> Result<(), Error> {
    for (body, expected) in [
        ("Link:https://example.org/path", 0),
        ("\"https://example.org/path\"", 0),
        ("\"https://example.org/path[Site]\"", 1),
        ("\\https://example.org/path", 0),
        ("prefix\\https://example.org/path[Site]", 0),
        ("https://example.org/path", 1),
    ] {
        for newline in ["\n", "\r\n"] {
            let source = format!("[source,text,subs=\"+macros\"]\n----\nα {body}\n----")
                .replace('\n', newline);
            let parsed = parse(&source, &Options::default())?;
            let [Block::DelimitedBlock(block)] = parsed.document().blocks.as_slice() else {
                return Err("expected a listing".into());
            };
            let DelimitedBlockType::DelimitedListing(nodes) = &block.inner else {
                return Err("expected listing content".into());
            };
            let mut links = Vec::new();
            collect_links(nodes, &mut links);
            assert_eq!(links.len(), expected, "{source:?}");
            for (_, location) in links {
                let span = &parsed.source()[location.absolute_start..=location.absolute_end];
                assert!(span.starts_with("https://"), "{span:?}");
                assert_eq!(location.start.line, 3);
            }
        }
    }
    Ok(())
}
