use acdc_parser::{InlineMacro, InlineNode, Options, parse, parse_inline};

type Error = Box<dyn std::error::Error>;

fn formatted_content<'source>(
    node: &InlineNode<'source>,
) -> Result<Vec<InlineNode<'source>>, Error> {
    if let InlineNode::BoldText(mut span) = node.clone() {
        Ok(span.take_content())
    } else if let InlineNode::ItalicText(mut span) = node.clone() {
        Ok(span.take_content())
    } else if let InlineNode::MonospaceText(mut span) = node.clone() {
        Ok(span.take_content())
    } else if let InlineNode::HighlightText(mut span) = node.clone() {
        Ok(span.take_content())
    } else {
        Err(format!("expected formatting, got {node:?}").into())
    }
}

#[test]
fn link_formatting_boundaries_register_nested_notes_once() -> Result<(), Error> {
    let source = include_str!("../../fixtures/tests/link_formatting_boundaries.adoc");
    let parsed = parse(source, &Options::default())?;
    let mut definitions = parsed.document().footnotes();
    let (Some(note), None) = (definitions.next(), definitions.next()) else {
        return Err("expected one retained note definition".into());
    };
    assert_eq!(note.id(), Some("one"));
    assert_eq!(note.number(), 1);
    assert!(!note.content().is_empty());
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
    let parsed_inlines = acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
    let [outer] = parsed_inlines.as_slice() else {
        return Err(format!(
            "expected one complete formatted span: {:?}",
            parsed_inlines.as_slice()
        )
        .into());
    };
    let original_content = formatted_content(outer)?;
    let [InlineNode::PlainText(_), node, InlineNode::PlainText(tail)] = original_content.as_slice()
    else {
        return Err(format!("expected a link inside the outer formatting: {outer:?}").into());
    };
    assert_eq!(acdc_parser::ast::PlainRef::from(tail).text(), " end");
    let location = node.location();
    assert_eq!(
        &source[location.absolute_start..=location.absolute_end],
        link
    );
    let text = if let InlineNode::Macro(InlineMacro::UrlMacro(link)) = node {
        link.clone().take_text()
    } else if let InlineNode::Macro(InlineMacro::Link(link)) = node {
        link.clone().take_text()
    } else if let InlineNode::Macro(InlineMacro::Mailto(link)) = node {
        link.clone().take_text()
    } else if let InlineNode::Macro(InlineMacro::CrossReference(link)) = node {
        link.clone().take_text()
    } else {
        return Err(format!("expected a complete link: {node:?}").into());
    };
    let [inner, InlineNode::PlainText(after)] = text.as_slice() else {
        return Err(format!("expected independently formatted label: {text:?}").into());
    };
    assert_eq!(acdc_parser::ast::PlainRef::from(after).text(), " after");
    assert_eq!(std::mem::discriminant(inner), std::mem::discriminant(outer));
    let location = inner.location();
    assert_eq!(
        &source[location.absolute_start..=location.absolute_end],
        format!("{marker}café{marker}")
    );
    Ok(())
}
