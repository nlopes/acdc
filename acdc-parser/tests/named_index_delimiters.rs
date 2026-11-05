use acdc_parser::{
    InlineMacro, InlineNode, Options,
    ast::{InlineKind, InlineRef},
    parse, parse_inline,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn named_index_delimiters_register_notes_and_anchors_once() -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/named_index_delimiters.adoc");
    let parsed = parse(source, &Options::default())?;
    let mut definitions = parsed.document().footnotes();
    let (Some(note), None) = (definitions.next(), definitions.next()) else {
        return Err("expected one footnote definition".into());
    };
    assert_eq!(note.id(), Some("one"));
    let mut content = note.content().inlines().map(InlineRef::kind);
    let (Some(InlineKind::Plain(body)), None) = (content.next(), content.next()) else {
        return Err("expected complete footnote body".into());
    };
    assert_eq!(body.text(), "Only note.");
    assert!(parsed.document().references().contains("inner"));
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[test]
fn named_index_delimiters_preserve_registration_time_macro_text() -> Result<(), Error> {
    let label = "Before https://example.org[Link] after";
    let source = format!("indexterm2:[{label}]");
    let parsed = parse_inline(&source, &Options::default())?;
    let parsed_inlines = acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
    let [InlineNode::Macro(InlineMacro::IndexTerm(term))] = parsed_inlines.as_slice() else {
        return Err("expected index term".into());
    };
    let content = acdc_parser::ContentAttribute::from(
        acdc_parser::ast::IndexTermRef::from(term.as_ref())
            .catalog()
            .term(),
    )
    .into_inlines();
    let [InlineNode::RawText(catalog)] = content.as_slice() else {
        return Err("expected literal catalog text".into());
    };
    let catalog = acdc_parser::ast::RawRef::from(catalog);
    assert_eq!(catalog.text(), label);
    assert_eq!(
        catalog.substitutions().collect::<Vec<_>>(),
        [acdc_parser::Substitution::SpecialChars]
    );
    assert_eq!(
        &source[catalog.location().absolute_start()..=catalog.location().absolute_end()],
        label
    );
    Ok(())
}

#[test]
fn named_index_delimiters_register_only_visible_targets() -> Result<(), Error> {
    let source =
        "indexterm2:[anchor:visible[]Shown] indexterm:[anchor:hidden[]Hidden]\n\n<<visible>>.";
    let parsed = parse(source, &Options::default())?;
    assert!(parsed.document().references().contains("visible"));
    assert!(!parsed.document().references().contains("hidden"));
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
    let parsed_inlines = acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
    let [
        _,
        InlineNode::Macro(InlineMacro::IndexTerm(term)),
        InlineNode::PlainText(tail),
    ] = parsed_inlines.as_slice()
    else {
        return Err(format!(
            "expected complete index term: {:?}",
            parsed_inlines.as_slice()
        )
        .into());
    };
    assert_eq!(acdc_parser::ast::PlainRef::from(tail).text(), " ω");
    assert_eq!(
        &source[acdc_parser::ast::IndexTermRef::from(term.as_ref())
            .location()
            .absolute_start()
            ..=acdc_parser::ast::IndexTermRef::from(term.as_ref())
                .location()
                .absolute_end()],
        format!("{outer}[{label}]")
    );
    let content = acdc_parser::ContentAttribute::from(
        acdc_parser::ast::IndexTermRef::from(term.as_ref()).term(),
    )
    .into_inlines();
    let [InlineNode::PlainText(_), node, InlineNode::PlainText(after)] = content.as_slice() else {
        return Err(format!("expected complete child macro: {:?}", content.as_slice()).into());
    };
    assert_eq!(acdc_parser::ast::PlainRef::from(after).text(), " after");
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
