use acdc_parser::{InlineMacro, InlineNode, Options, parse_inline};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn unexpected(message: &str, actual: impl std::fmt::Debug) -> Box<dyn std::error::Error> {
    std::io::Error::other(format!("{message}, got {actual:?}")).into()
}

fn assert_plain_text(text: &[InlineNode<'_>], expected: &str) -> TestResult {
    let [InlineNode::PlainText(text)] = text else {
        return Err(unexpected("expected plain macro text", text));
    };
    assert_eq!(text.content, expected);
    Ok(())
}

#[test]
fn attribute_only_link_role_is_not_display_text() -> TestResult {
    let parsed = parse_inline(
        "link:https://example.com[role=include]",
        &Options::default(),
    )?;
    let [InlineNode::Macro(InlineMacro::Link(link))] = parsed.inlines() else {
        return Err(unexpected("expected one link macro", parsed.inlines()));
    };

    assert_eq!(link.target.to_string(), "https://example.com");
    assert!(link.text.is_empty());
    assert_eq!(link.attributes.iter().count(), 1);
    assert_eq!(
        link.attributes.get_string("role").as_deref(),
        Some("include")
    );

    Ok(())
}

#[test]
fn attribute_only_url_role_is_not_display_text() -> TestResult {
    let parsed = parse_inline("https://example.com[role=include]", &Options::default())?;
    let [InlineNode::Macro(InlineMacro::Url(url))] = parsed.inlines() else {
        return Err(unexpected("expected one URL macro", parsed.inlines()));
    };

    assert_eq!(url.target.to_string(), "https://example.com");
    assert!(url.text.is_empty());
    assert_eq!(url.attributes.iter().count(), 1);
    assert_eq!(
        url.attributes.get_string("role").as_deref(),
        Some("include")
    );

    Ok(())
}

#[test]
fn attribute_like_mailto_content_remains_display_text() -> TestResult {
    let parsed = parse_inline("mailto:joe@example.com[role=include]", &Options::default())?;
    let [InlineNode::Macro(InlineMacro::Mailto(mailto))] = parsed.inlines() else {
        return Err(unexpected("expected one mailto macro", parsed.inlines()));
    };

    assert_eq!(mailto.target.to_string(), "mailto:joe@example.com");
    assert_plain_text(&mailto.text, "role=include")?;
    assert!(mailto.attributes.is_empty());

    Ok(())
}

#[test]
fn quoted_named_attribute_syntax_remains_link_text() -> TestResult {
    let parsed = parse_inline(
        "link:https://example.com[\"role=include\"]",
        &Options::default(),
    )?;
    let [InlineNode::Macro(InlineMacro::Link(link))] = parsed.inlines() else {
        return Err(unexpected("expected one link macro", parsed.inlines()));
    };

    assert_plain_text(&link.text, "role=include")?;
    assert!(link.attributes.is_empty());

    Ok(())
}

#[test]
fn link_with_text_and_attributes_parses_both() -> TestResult {
    let parsed = parse_inline(
        "link:https://example.com[Example,role=external,window=_blank]",
        &Options::default(),
    )?;
    let [InlineNode::Macro(InlineMacro::Link(link))] = parsed.inlines() else {
        return Err(unexpected("expected one link macro", parsed.inlines()));
    };

    assert_eq!(link.target.to_string(), "https://example.com");
    assert_plain_text(&link.text, "Example")?;
    assert_eq!(link.attributes.iter().count(), 2);
    assert_eq!(
        link.attributes.get_string("role").as_deref(),
        Some("external")
    );
    assert_eq!(
        link.attributes.get_string("window").as_deref(),
        Some("_blank")
    );

    Ok(())
}

#[test]
fn url_with_text_and_attributes_parses_both() -> TestResult {
    let parsed = parse_inline(
        "https://example.com[Example,role=external,window=_blank]",
        &Options::default(),
    )?;
    let [InlineNode::Macro(InlineMacro::Url(url))] = parsed.inlines() else {
        return Err(unexpected("expected one URL macro", parsed.inlines()));
    };

    assert_eq!(url.target.to_string(), "https://example.com");
    assert_plain_text(&url.text, "Example")?;
    assert_eq!(url.attributes.iter().count(), 2);
    assert_eq!(
        url.attributes.get_string("role").as_deref(),
        Some("external")
    );
    assert_eq!(
        url.attributes.get_string("window").as_deref(),
        Some("_blank")
    );

    Ok(())
}

#[test]
fn mailto_with_text_and_attributes_parses_both() -> TestResult {
    let parsed = parse_inline(
        "mailto:joe@example.com[Joe,role=include]",
        &Options::default(),
    )?;
    let [InlineNode::Macro(InlineMacro::Mailto(mailto))] = parsed.inlines() else {
        return Err(unexpected("expected one mailto macro", parsed.inlines()));
    };

    assert_eq!(mailto.target.to_string(), "mailto:joe@example.com");
    assert_plain_text(&mailto.text, "Joe")?;
    assert_eq!(mailto.attributes.iter().count(), 1);
    assert_eq!(
        mailto.attributes.get_string("role").as_deref(),
        Some("include")
    );

    Ok(())
}
