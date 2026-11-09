use acdc_parser::{
    Options,
    ast::{BlockKind, ContentRef, InlineKind, InlineRef},
    parse, parse_inline,
};

type Error = Box<dyn std::error::Error>;

fn unexpected(message: &str, actual: impl std::fmt::Debug) -> Error {
    std::io::Error::other(format!("{message}, got {actual:?}")).into()
}

fn plain_text(inlines: ContentRef<'_>) -> Result<String, Error> {
    let mut text = String::new();
    for inline in inlines.inlines() {
        if let InlineKind::Plain(plain) = inline.kind() {
            text.push_str(plain.text());
        } else if let InlineKind::Raw(raw) = inline.kind() {
            text.push_str(raw.text());
        } else {
            return Err(unexpected("expected unformatted text", inlines));
        }
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
        .blocks()
        .filter_map(|block| {
            let BlockKind::Paragraph(paragraph) = block.kind() else {
                return None;
            };
            paragraph.content().inlines().find_map(|inline| {
                if let InlineKind::UrlMacro(url) = inline.kind() {
                    Some(url.hides_uri_scheme())
                } else if let InlineKind::Link(link) = inline.kind() {
                    Some(link.hides_uri_scheme())
                } else if let InlineKind::Autolink(link) = inline.kind() {
                    Some(link.hides_uri_scheme())
                } else {
                    None
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
        let [InlineKind::CrossReference(xref)] = &*parsed
            .content()
            .inlines()
            .map(InlineRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err(unexpected(
                "expected one cross-reference macro",
                parsed.content(),
            ));
        };

        assert!(
            xref.text()
                .inlines()
                .any(|inline| matches!(inline.kind(), InlineKind::Bold(_))),
            "expected formatted cross-reference text in {input:?}"
        );
        assert!(
            xref.text()
                .inlines()
                .any(|inline| matches!(inline.kind(), InlineKind::Italic(_))),
            "expected italic cross-reference text in {input:?}"
        );
        assert!(
            xref.text()
                .inlines()
                .any(|inline| matches!(inline.kind(), InlineKind::Monospace(_))),
            "expected monospace cross-reference text in {input:?}"
        );
        let Some(link) = xref.text().inlines().find_map(|inline| {
            if let InlineKind::Link(link) = inline.kind() {
                Some((link.target().to_string(), link.text()))
            } else if let InlineKind::UrlMacro(url) = inline.kind() {
                Some((url.target().to_string(), url.text()))
            } else {
                None
            }
        }) else {
            return Err(unexpected(
                "expected a nested link or URL macro",
                xref.text(),
            ));
        };
        assert_eq!(link.0, "https://example.com");
        let [InlineKind::Plain(text)] = &*link.1.inlines().map(InlineRef::kind).collect::<Vec<_>>()
        else {
            return Err(unexpected("expected plain nested link text", link.1));
        };
        assert_eq!(text.text(), "link");
        let Some(InlineKind::Plain(tail)) = xref.text().inlines().next_back().map(InlineRef::kind)
        else {
            return Err(unexpected(
                "expected trailing cross-reference text",
                xref.text(),
            ));
        };
        assert_eq!(tail.text(), " tail");
    }

    Ok(())
}

#[test]
fn xref_text_accepts_protected_inline_macro_forms() -> Result<(), Error> {
    let options = Options::builder()
        .with_attribute("experimental", true)
        .build()?;
    for (nested, expected_kind) in [
        ("footnote:[note]", "footnote"),
        ("footnote:named[note]", "footnote"),
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
        let [InlineKind::CrossReference(xref)] = &*parsed
            .content()
            .inlines()
            .map(InlineRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err(unexpected(
                "expected one cross-reference macro",
                parsed.content(),
            ));
        };
        assert!(
            matches!(xref.text().inlines().next().map(InlineRef::kind), Some(InlineKind::Plain(text)) if text.text() == "before "),
            "expected leading text around {nested:?}, got {:?}",
            xref.text()
        );
        assert!(
            matches!(xref.text().inlines().next_back().map(InlineRef::kind), Some(InlineKind::Plain(text)) if text.text() == " after"),
            "expected trailing text around {nested:?}, got {:?}",
            xref.text()
        );
        assert!(
            xref.text().inlines().any(|inline| {
                matches!(
                    (expected_kind, inline.kind()),
                    ("anchor", InlineKind::Anchor(_))
                        | ("footnote", InlineKind::Footnote(_))
                        | ("raw", InlineKind::Raw(_))
                        | ("mailto", InlineKind::Mailto(_))
                        | ("image", InlineKind::Image(_))
                        | ("icon", InlineKind::Icon(_))
                        | ("indexterm", InlineKind::IndexTerm(_))
                        | ("stem", InlineKind::Stem(_))
                        | ("keyboard", InlineKind::KeyboardShortcut(_))
                        | ("button", InlineKind::Button(_))
                        | ("menu", InlineKind::MenuPath(_))
                )
            }),
            "expected a nested {expected_kind} macro in {:?}",
            xref.text()
        );
    }
    Ok(())
}

#[test]
fn xref_macro_honors_closing_bracket_boundaries() -> Result<(), Error> {
    let parsed = parse_inline(r"xref:target[literal \] tail]", &Options::default())?;
    let [InlineKind::CrossReference(xref)] = &*parsed
        .content()
        .inlines()
        .map(InlineRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err(unexpected(
            "expected one cross-reference macro",
            parsed.content(),
        ));
    };
    assert_eq!(plain_text(xref.text())?, "literal ] tail");

    for (nested, expected_xref_text) in [
        ("[bracket]", "literal [bracket"),
        ("xref:target[inner]", "literal xref:target[inner"),
    ] {
        let input = format!("xref:target[literal {nested} tail]");
        let parsed = parse_inline(&input, &Options::default())?;
        let [InlineKind::CrossReference(xref), InlineKind::Plain(tail)] = &*parsed
            .content()
            .inlines()
            .map(InlineRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err(unexpected(
                "expected one cross-reference and trailing text",
                parsed.content(),
            ));
        };
        assert_eq!(plain_text(xref.text())?, expected_xref_text);
        assert_eq!(tail.text(), " tail]");
    }
    Ok(())
}
