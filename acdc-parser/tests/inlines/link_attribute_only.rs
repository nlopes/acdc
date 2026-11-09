use acdc_parser::{InlineMacro, InlineNode, Options, parse_inline};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn unexpected(message: &str, actual: impl std::fmt::Debug) -> Box<dyn std::error::Error> {
    std::io::Error::other(format!("{message}, got {actual:?}")).into()
}

fn assert_plain_text(text: &[InlineNode<'_>], expected: &str) -> TestResult {
    let [InlineNode::PlainText(text)] = text else {
        return Err(unexpected("expected plain macro text", text));
    };
    assert_eq!(acdc_parser::ast::PlainRef::from(text).text(), expected);
    Ok(())
}

#[test]
fn attribute_only_link_role_is_not_display_text() -> TestResult {
    let parsed = parse_inline(
        "link:https://example.com[role=include]",
        &Options::default(),
    )?;
    let parsed_inlines = acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
    let [InlineNode::Macro(InlineMacro::Link(link))] = parsed_inlines.as_slice() else {
        return Err(unexpected(
            "expected one link macro",
            parsed_inlines.as_slice(),
        ));
    };

    assert_eq!(
        acdc_parser::ast::LinkRef::from(link.as_ref())
            .target()
            .to_string(),
        "https://example.com"
    );
    assert_eq!(link.clone().take_text(), []);
    assert_eq!(link.clone_attributes().iter().count(), 0);
    assert_eq!(
        link.clone_metadata()
            .roles
            .into_tokens()
            .unwrap_or_default(),
        ["include"]
    );

    Ok(())
}

#[test]
fn attribute_only_url_role_is_not_display_text() -> TestResult {
    let parsed = parse_inline("https://example.com[role=include]", &Options::default())?;
    let parsed_inlines = acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
    let [InlineNode::Macro(InlineMacro::UrlMacro(url))] = parsed_inlines.as_slice() else {
        return Err(unexpected(
            "expected one URL macro",
            parsed_inlines.as_slice(),
        ));
    };

    assert_eq!(
        acdc_parser::ast::UrlMacroRef::from(url.as_ref())
            .target()
            .to_string(),
        "https://example.com"
    );
    assert_eq!(url.clone().take_text(), []);
    assert_eq!(url.clone_attributes().iter().count(), 0);
    assert_eq!(
        url.clone_metadata().roles.into_tokens().unwrap_or_default(),
        ["include"]
    );

    Ok(())
}

#[test]
fn attribute_like_mailto_content_remains_display_text() -> TestResult {
    let parsed = parse_inline("mailto:joe@example.com[role=include]", &Options::default())?;
    let parsed_inlines = acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
    let [InlineNode::Macro(InlineMacro::Mailto(mailto))] = parsed_inlines.as_slice() else {
        return Err(unexpected(
            "expected one mailto macro",
            parsed_inlines.as_slice(),
        ));
    };

    assert_eq!(
        acdc_parser::ast::MailtoRef::from(mailto.as_ref())
            .target()
            .to_string(),
        "mailto:joe@example.com"
    );
    assert_plain_text(&mailto.clone().take_text(), "role=include")?;
    assert!(mailto.clone_attributes().is_empty());

    Ok(())
}

#[test]
fn quoted_named_attribute_syntax_remains_link_text() -> TestResult {
    let parsed = parse_inline(
        "link:https://example.com[\"role=include\"]",
        &Options::default(),
    )?;
    let parsed_inlines = acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
    let [InlineNode::Macro(InlineMacro::Link(link))] = parsed_inlines.as_slice() else {
        return Err(unexpected(
            "expected one link macro",
            parsed_inlines.as_slice(),
        ));
    };

    assert_plain_text(&link.clone().take_text(), "role=include")?;
    assert!(link.clone_attributes().is_empty());

    Ok(())
}

#[test]
fn positional_value_without_named_attribute_remains_link_text() -> TestResult {
    let parsed = parse_inline(
        "link:https://example.com[Example,positional-id]",
        &Options::default(),
    )?;
    let parsed_inlines = acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
    let [InlineNode::Macro(InlineMacro::Link(link))] = parsed_inlines.as_slice() else {
        return Err(unexpected(
            "expected one link macro",
            parsed_inlines.as_slice(),
        ));
    };

    assert_plain_text(&link.clone().take_text(), "Example,positional-id")?;
    assert!(link.clone_attributes().is_empty());

    Ok(())
}

#[test]
fn link_with_text_and_attributes_parses_both() -> TestResult {
    let parsed = parse_inline(
        "link:https://example.com[Example,role=external,window=_blank]",
        &Options::default(),
    )?;
    let parsed_inlines = acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
    let [InlineNode::Macro(InlineMacro::Link(link))] = parsed_inlines.as_slice() else {
        return Err(unexpected(
            "expected one link macro",
            parsed_inlines.as_slice(),
        ));
    };

    assert_eq!(
        acdc_parser::ast::LinkRef::from(link.as_ref())
            .target()
            .to_string(),
        "https://example.com"
    );
    assert_plain_text(&link.clone().take_text(), "Example")?;
    assert_eq!(link.clone_attributes().iter().count(), 1);
    assert_eq!(
        link.clone_metadata()
            .roles
            .into_tokens()
            .unwrap_or_default(),
        ["external"]
    );
    assert_eq!(
        link.clone_attributes().get_string("window").as_deref(),
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
    let parsed_inlines = acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
    let [InlineNode::Macro(InlineMacro::UrlMacro(url))] = parsed_inlines.as_slice() else {
        return Err(unexpected(
            "expected one URL macro",
            parsed_inlines.as_slice(),
        ));
    };

    assert_eq!(
        acdc_parser::ast::UrlMacroRef::from(url.as_ref())
            .target()
            .to_string(),
        "https://example.com"
    );
    assert_plain_text(&url.clone().take_text(), "Example")?;
    assert_eq!(url.clone_attributes().iter().count(), 1);
    assert_eq!(
        url.clone_metadata().roles.into_tokens().unwrap_or_default(),
        ["external"]
    );
    assert_eq!(
        url.clone_attributes().get_string("window").as_deref(),
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
    let parsed_inlines = acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
    let [InlineNode::Macro(InlineMacro::Mailto(mailto))] = parsed_inlines.as_slice() else {
        return Err(unexpected(
            "expected one mailto macro",
            parsed_inlines.as_slice(),
        ));
    };

    assert_eq!(
        acdc_parser::ast::MailtoRef::from(mailto.as_ref())
            .target()
            .to_string(),
        "mailto:joe@example.com"
    );
    assert_plain_text(&mailto.clone().take_text(), "Joe")?;
    assert_eq!(mailto.clone_attributes().iter().count(), 0);
    assert_eq!(
        mailto
            .clone_metadata()
            .roles
            .into_tokens()
            .unwrap_or_default(),
        ["include"]
    );

    Ok(())
}
