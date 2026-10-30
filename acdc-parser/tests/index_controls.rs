use acdc_parser::{
    CompatibilityMode, IndexTerm, InlineMacro, InlineNode, Options, parse, parse_inline,
};

type Error = Box<dyn std::error::Error>;

fn index<'a>(nodes: &'a [InlineNode<'a>]) -> Result<&'a IndexTerm<'a>, Error> {
    match nodes {
        [InlineNode::Macro(InlineMacro::IndexTerm(term))] => Ok(term),
        _ => Err(format!("expected one index term: {nodes:?}").into()),
    }
}

fn text(nodes: &[InlineNode<'_>]) -> Result<String, Error> {
    nodes
        .iter()
        .map(|node| {
            let InlineNode::PlainText(node) = node else {
                return Err(format!("expected plain label: {node:?}").into());
            };
            Ok(node.content)
        })
        .collect()
}

#[rstest::rstest]
#[case("indexterm2:[Visible,custom=kept]", "Visible")]
#[case("indexterm2:[,custom=kept]", ",custom=kept")]
#[case("indexterm2:[\"Visible, quoted\",custom=\"a,b\"]", "Visible, quoted")]
#[case("indexterm2:[Visible,role=red,id=entry,opts=\"a,b\"]", "Visible")]
#[case("indexterm2:[Visible,Secondary]", "Visible,Secondary")]
#[case("indexterm2:[custom=kept,Visible]", "custom=kept,Visible")]
#[case("((Visible,custom=literal))", "Visible,custom=literal")]
fn named_index_controls_stay_out_of_visible_labels(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] source: &str,
    #[case] expected: &str,
) -> Result<(), Error> {
    let parsed = parse_inline(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let term = index(parsed.inlines())?;
    assert!(term.is_visible());
    assert_eq!(text(term.term())?, expected);
    assert_eq!(text(term.catalog_entry().term())?, expected);
    Ok(())
}

#[rstest::rstest]
#[case("indexterm:[Primary,custom=kept,Secondary,Tertiary]", None, None)]
#[case("indexterm:[Primary,,Tertiary,custom=kept]", None, None)]
#[case(
    "indexterm:[Primary,Secondary,Tertiary,custom=kept]",
    Some("Secondary"),
    Some("Tertiary")
)]
#[case(
    "indexterm:[Primary,\"\",Tertiary,custom=kept]",
    Some(""),
    Some("Tertiary")
)]
#[case("(((Primary,custom=literal)))", Some("custom=literal"), None)]
fn index_hierarchy_uses_physical_slots(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] source: &str,
    #[case] secondary: Option<&str>,
    #[case] tertiary: Option<&str>,
) -> Result<(), Error> {
    let parsed = parse_inline(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let term = index(parsed.inlines())?;
    assert!(!term.is_visible());
    assert_eq!(text(term.term())?, "Primary");
    assert_eq!(
        term.secondary().map(text).transpose()?.as_deref(),
        secondary
    );
    assert_eq!(term.tertiary().map(text).transpose()?.as_deref(), tertiary);
    Ok(())
}

#[test]
fn discarded_index_controls_and_relationships_do_not_register_footnotes() -> Result<(), Error> {
    let parsed = parse(
        "indexterm2:[Visible,custom=footnote:discarded[Unused]]\n\n\
         indexterm2:[Visible,see=Target,see-also=\"footnote:ignored[Unused]\"]\n\n\
         indexterm2:[Selected footnote:used[Only note.],custom=kept]",
        &Options::default(),
    )?;
    let [note] = parsed.document().footnotes.as_slice() else {
        return Err(format!(
            "expected one selected note: {:?}",
            parsed.document().footnotes
        )
        .into());
    };
    assert_eq!(note.id, Some("used"));
    assert_eq!(text(&note.content)?, "Only note.");
    Ok(())
}

#[test]
fn named_index_controls_preserve_complete_child_labels() -> Result<(), Error> {
    let source = "indexterm2:[Before link:guide.adoc[Child,role=child] after,custom=outer]";
    let parsed = parse_inline(source, &Options::default())?;
    let term = index(parsed.inlines())?;
    let [
        InlineNode::PlainText(before),
        InlineNode::Macro(InlineMacro::Link(link)),
        InlineNode::PlainText(after),
    ] = term.term()
    else {
        return Err(format!("expected complete child link: {:?}", term.term()).into());
    };
    assert_eq!(before.content, "Before ");
    assert_eq!(text(&link.text)?, "Child");
    assert_eq!(link.metadata.roles, ["child"]);
    assert_eq!(after.content, " after");
    assert_eq!(
        &source[link.location.absolute_start..=link.location.absolute_end],
        "link:guide.adoc[Child,role=child]"
    );
    Ok(())
}
