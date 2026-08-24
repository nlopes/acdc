use acdc_parser::{Block, InlineMacro, InlineNode, Options, parse, parse_inline};

type Error = Box<dyn std::error::Error>;

fn unexpected(message: &str, actual: impl std::fmt::Debug) -> Error {
    std::io::Error::other(format!("{message}, got {actual:?}")).into()
}

fn plain_text(inlines: &[InlineNode<'_>]) -> Result<String, Error> {
    let mut text = String::new();
    for inline in inlines {
        let InlineNode::PlainText(plain) = inline else {
            return Err(unexpected("expected plain text", inlines));
        };
        text.push_str(plain.content);
    }
    Ok(text)
}

#[test]
fn hide_uri_scheme_is_resolved_at_each_links_source_position() -> Result<(), Error> {
    let parsed = parse(
        "https://before.example[]\n\n:hide-uri-scheme:\n\nhttps://during.example\n\n:hide-uri-scheme: false\n\nlink:https://false.example[]\n\n:hide-uri-scheme!:\n\nhttps://after.example[]\n\n.Local\n:hide-uri-scheme:\nlink:https://local.example[]\n",
        &Options::default(),
    )?;

    let actual = parsed
        .document()
        .blocks
        .iter()
        .filter_map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return None;
            };
            paragraph.content.iter().find_map(|inline| {
                let InlineNode::Macro(inline) = inline else {
                    return None;
                };
                match inline {
                    InlineMacro::Url(url) => Some(url.hides_uri_scheme()),
                    InlineMacro::Link(link) => Some(link.hides_uri_scheme()),
                    InlineMacro::Autolink(autolink) => Some(autolink.hides_uri_scheme()),
                    InlineMacro::Footnote(_)
                    | InlineMacro::Icon(_)
                    | InlineMacro::Image(_)
                    | InlineMacro::Keyboard(_)
                    | InlineMacro::Button(_)
                    | InlineMacro::Menu(_)
                    | InlineMacro::Mailto(_)
                    | InlineMacro::CrossReference(_)
                    | InlineMacro::Pass(_)
                    | InlineMacro::Stem(_)
                    | InlineMacro::IndexTerm(_)
                    | _ => None,
                }
            })
        })
        .collect::<Vec<_>>();

    assert_eq!(actual, [false, true, true, false, true]);
    Ok(())
}

#[test]
fn xref_text_accepts_a_nested_link_macro() -> Result<(), Error> {
    for input in [
        "xref:target[Own *bold* _italic_ `mono` https://example.com[link] tail]",
        "<<target,Own *bold* _italic_ `mono` link:https://example.com[link] tail>>",
    ] {
        let parsed = parse_inline(input, &Options::default())?;
        let [InlineNode::Macro(InlineMacro::CrossReference(xref))] = parsed.inlines() else {
            return Err(unexpected(
                "expected one cross-reference macro",
                parsed.inlines(),
            ));
        };

        assert!(
            xref.text
                .iter()
                .any(|inline| matches!(inline, InlineNode::BoldText(_))),
            "expected formatted cross-reference text in {input:?}"
        );
        assert!(
            xref.text
                .iter()
                .any(|inline| matches!(inline, InlineNode::ItalicText(_))),
            "expected italic cross-reference text in {input:?}"
        );
        assert!(
            xref.text
                .iter()
                .any(|inline| matches!(inline, InlineNode::MonospaceText(_))),
            "expected monospace cross-reference text in {input:?}"
        );
        let Some(link) = xref.text.iter().find_map(|inline| {
            let InlineNode::Macro(inline) = inline else {
                return None;
            };
            match inline {
                InlineMacro::Link(link) => Some((link.target.to_string(), link.text.as_slice())),
                InlineMacro::Url(url) => Some((url.target.to_string(), url.text.as_slice())),
                _ => None,
            }
        }) else {
            return Err(unexpected(
                "expected a nested link or URL macro",
                &xref.text,
            ));
        };
        assert_eq!(link.0, "https://example.com");
        let [InlineNode::PlainText(text)] = link.1 else {
            return Err(unexpected("expected plain nested link text", link.1));
        };
        assert_eq!(text.content, "link");
        let Some(InlineNode::PlainText(tail)) = xref.text.last() else {
            return Err(unexpected(
                "expected trailing cross-reference text",
                &xref.text,
            ));
        };
        assert_eq!(tail.content, " tail");
    }

    Ok(())
}

#[test]
fn xref_text_accepts_protected_inline_macro_forms() -> Result<(), Error> {
    let options = Options::builder()
        .with_attribute("experimental", true)
        .build();
    for (nested, expected_kind) in [
        ("mailto:user@example.com[mail]", "mailto"),
        ("image:missing.png[alt]", "image"),
        ("icon:heart[]", "icon"),
        ("[[inner]]", "anchor"),
        ("indexterm:[Term]", "indexterm"),
        ("pass:[literal]", "raw"),
        ("stem:[x + y]", "stem"),
        ("kbd:[Ctrl+C]", "keyboard"),
        ("btn:[Save]", "button"),
        ("menu:File[Open]", "menu"),
    ] {
        let input = format!("xref:target[before {nested} after]");
        let parsed = parse_inline(&input, &options)?;
        let [InlineNode::Macro(InlineMacro::CrossReference(xref))] = parsed.inlines() else {
            return Err(unexpected(
                "expected one cross-reference macro",
                parsed.inlines(),
            ));
        };
        assert!(
            matches!(xref.text.first(), Some(InlineNode::PlainText(text)) if text.content == "before "),
            "expected leading text around {nested:?}, got {:?}",
            xref.text
        );
        assert!(
            matches!(xref.text.last(), Some(InlineNode::PlainText(text)) if text.content == " after"),
            "expected trailing text around {nested:?}, got {:?}",
            xref.text
        );
        assert!(
            xref.text
                .iter()
                .any(|inline| match (expected_kind, inline) {
                    ("anchor", InlineNode::InlineAnchor(_)) | ("raw", InlineNode::RawText(_)) =>
                        true,
                    ("mailto", InlineNode::Macro(InlineMacro::Mailto(_)))
                    | ("image", InlineNode::Macro(InlineMacro::Image(_)))
                    | ("icon", InlineNode::Macro(InlineMacro::Icon(_)))
                    | ("indexterm", InlineNode::Macro(InlineMacro::IndexTerm(_)))
                    | ("stem", InlineNode::Macro(InlineMacro::Stem(_)))
                    | ("keyboard", InlineNode::Macro(InlineMacro::Keyboard(_)))
                    | ("button", InlineNode::Macro(InlineMacro::Button(_)))
                    | ("menu", InlineNode::Macro(InlineMacro::Menu(_))) => true,
                    _ => false,
                }),
            "expected a nested {expected_kind} macro in {:?}",
            xref.text
        );
    }
    Ok(())
}

#[test]
fn xref_macro_honors_closing_bracket_boundaries() -> Result<(), Error> {
    let parsed = parse_inline(r"xref:target[literal \] tail]", &Options::default())?;
    let [InlineNode::Macro(InlineMacro::CrossReference(xref))] = parsed.inlines() else {
        return Err(unexpected(
            "expected one cross-reference macro",
            parsed.inlines(),
        ));
    };
    assert_eq!(plain_text(&xref.text)?, "literal ] tail");

    for (nested, expected_xref_text) in [
        ("[bracket]", "literal [bracket"),
        ("xref:target[inner]", "literal xref:target[inner"),
        ("footnote:[note]", "literal footnote:[note"),
    ] {
        let input = format!("xref:target[literal {nested} tail]");
        let parsed = parse_inline(&input, &Options::default())?;
        let [
            InlineNode::Macro(InlineMacro::CrossReference(xref)),
            InlineNode::PlainText(tail),
        ] = parsed.inlines()
        else {
            return Err(unexpected(
                "expected one cross-reference and trailing text",
                parsed.inlines(),
            ));
        };
        assert_eq!(plain_text(&xref.text)?, expected_xref_text);
        assert_eq!(tail.content, " tail]");
    }
    Ok(())
}
