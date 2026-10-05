use acdc_parser::{InlineMacro, InlineNode, Options, parse, parse_inline};

type Error = Box<dyn std::error::Error>;

#[test]
fn named_index_delimiters_register_notes_and_anchors_once() -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/named_index_delimiters.adoc");
    let parsed = parse(source, &Options::default())?;
    let [note] = parsed.document().footnotes.as_slice() else {
        return Err("expected one footnote definition".into());
    };
    assert_eq!(note.id, Some("one"));
    let [InlineNode::PlainText(body)] = note.content.as_slice() else {
        return Err("expected complete footnote body".into());
    };
    assert_eq!(body.content, "Only note.");
    assert!(parsed.document().references.contains_key("inner"));
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[test]
fn named_index_delimiters_preserve_registration_time_macro_text() -> Result<(), Error> {
    let label = "Before https://example.org[Link] after";
    let source = format!("indexterm2:[{label}]");
    let parsed = parse_inline(&source, &Options::default())?;
    let [InlineNode::Macro(InlineMacro::IndexTerm(term))] = parsed.inlines() else {
        return Err("expected index term".into());
    };
    let [InlineNode::PlainText(catalog)] = term.catalog_entry().term() else {
        return Err("expected literal catalog text".into());
    };
    assert_eq!(catalog.content, label);
    assert_eq!(
        &source[catalog.location.absolute_start..=catalog.location.absolute_end],
        label
    );
    Ok(())
}

#[test]
fn named_index_delimiters_register_only_visible_targets() -> Result<(), Error> {
    let source =
        "indexterm2:[anchor:visible[]Shown] indexterm:[anchor:hidden[]Hidden]\n\n<<visible>>.";
    let parsed = parse(source, &Options::default())?;
    assert!(parsed.document().references.contains_key("visible"));
    assert!(!parsed.document().references.contains_key("hidden"));
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn named_index_delimiters_preserve_complete_children_and_source_spans(
    #[values("indexterm2:", "indexterm:")] outer: &str,
    #[values(
        "https://example.org[*café*]",
        "link:manual.html[café]",
        "mailto:a@example.org[café]",
        "xref:target[café]",
        "<<target,café] text>>",
        "footnote:one[café]",
        "anchor:inner[café]",
        "image:missing.png[café]",
        "icon:heart[café]",
        "https://example.org[outer mailto:a@example.org[inner] tail]"
    )]
    child: &str,
) -> Result<(), Error> {
    let label = format!("Before {child} after");
    let label = if outer == "indexterm:" {
        format!("\"{label}\"")
    } else {
        label
    };
    let source = format!("α {outer}[{label}] ω");
    let parsed = parse_inline(&source, &Options::default())?;
    let [
        _,
        InlineNode::Macro(InlineMacro::IndexTerm(term)),
        InlineNode::PlainText(tail),
    ] = parsed.inlines()
    else {
        return Err(format!("expected complete index term: {:?}", parsed.inlines()).into());
    };
    assert_eq!(tail.content, " ω");
    assert_eq!(
        &source[term.location.absolute_start..=term.location.absolute_end],
        format!("{outer}[{label}]")
    );
    let [InlineNode::PlainText(_), node, InlineNode::PlainText(after)] = term.term() else {
        return Err(format!("expected complete child macro: {:?}", term.term()).into());
    };
    assert_eq!(after.content, " after");
    let location = node.location();
    assert_eq!(
        &source[location.absolute_start..=location.absolute_end],
        child
    );
    assert_eq!(
        location.start.column as usize,
        source[..location.absolute_start].chars().count() + 1
    );
    Ok(())
}
