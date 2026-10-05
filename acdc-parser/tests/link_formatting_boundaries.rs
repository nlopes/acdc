use acdc_parser::{InlineMacro, InlineNode, Options, parse, parse_inline};

type Error = Box<dyn std::error::Error>;

fn formatted_content<'node, 'source>(
    node: &'node InlineNode<'source>,
) -> Result<&'node [InlineNode<'source>], Error> {
    if let InlineNode::BoldText(span) = node {
        Ok(&span.content)
    } else if let InlineNode::ItalicText(span) = node {
        Ok(&span.content)
    } else if let InlineNode::MonospaceText(span) = node {
        Ok(&span.content)
    } else if let InlineNode::HighlightText(span) = node {
        Ok(&span.content)
    } else {
        Err(format!("expected formatting, got {node:?}").into())
    }
}

#[test]
fn link_formatting_boundaries_register_nested_notes_once() -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/link_formatting_boundaries.adoc");
    let parsed = parse(source, &Options::default())?;
    let [note] = parsed.document().footnotes.as_slice() else {
        return Err("expected one retained note definition".into());
    };
    assert_eq!(note.id, Some("one"));
    assert_eq!(note.number, 1);
    assert!(!note.content.is_empty());
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn link_formatting_boundaries_keep_complete_labels_and_source_spans(
    #[values("*", "_", "`", "#")] marker: &str,
    #[values(
        "https://example.org",
        "link:manual.html",
        "mailto:user@example.org",
        "xref:target",
        "<<target,"
    )]
    target: &str,
) -> Result<(), Error> {
    let label = format!("{marker}café{marker} after");
    let link = if target.starts_with("<<") {
        format!("{target}{label}>>")
    } else {
        format!("{target}[{label}]")
    };
    let source = format!("{marker}Before {link} end{marker}");
    let parsed = parse_inline(&source, &Options::default())?;
    let [outer] = parsed.inlines() else {
        return Err(format!(
            "expected one complete formatted span: {:?}",
            parsed.inlines()
        )
        .into());
    };
    let [InlineNode::PlainText(_), node, InlineNode::PlainText(tail)] = formatted_content(outer)?
    else {
        return Err(format!("expected a link inside the outer formatting: {outer:?}").into());
    };
    assert_eq!(tail.content, " end");
    let location = node.location();
    assert_eq!(
        &source[location.absolute_start..=location.absolute_end],
        link
    );
    let text = if let InlineNode::Macro(InlineMacro::Url(link)) = node {
        &link.text
    } else if let InlineNode::Macro(InlineMacro::Link(link)) = node {
        &link.text
    } else if let InlineNode::Macro(InlineMacro::Mailto(link)) = node {
        &link.text
    } else if let InlineNode::Macro(InlineMacro::CrossReference(link)) = node {
        &link.text
    } else {
        return Err(format!("expected a complete link: {node:?}").into());
    };
    let [inner, InlineNode::PlainText(after)] = text.as_slice() else {
        return Err(format!("expected independently formatted label: {text:?}").into());
    };
    assert_eq!(after.content, " after");
    assert_eq!(std::mem::discriminant(inner), std::mem::discriminant(outer));
    let location = inner.location();
    assert_eq!(
        &source[location.absolute_start..=location.absolute_end],
        format!("{marker}café{marker}")
    );
    Ok(())
}
