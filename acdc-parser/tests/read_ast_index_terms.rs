use acdc_parser::{
    CompatibilityMode, InlineMacro, InlineNode, Options,
    ast::{ContentRef, IndexTermRef, IndexTermRelationshipRef, InlineKind},
    parse, parse_inline,
};

type Error = Box<dyn std::error::Error>;

fn text(content: ContentRef<'_>) -> Result<String, Error> {
    content
        .inlines()
        .map(|inline| {
            if let InlineKind::Plain(node) = inline.kind() {
                Ok(node.text())
            } else if let InlineKind::Raw(node) = inline.kind() {
                Ok(node.text())
            } else if let InlineKind::Verbatim(node) = inline.kind() {
                Ok(node.text())
            } else {
                Err("expected text content".into())
            }
        })
        .collect()
}

fn shared<T: Copy + Send + Sync>(_: T) {}

#[rstest::rstest]
fn index_reads_preserve_hierarchy_and_relationships(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse_inline(
        "indexterm:[Primary,\"\",Tertiary,see-also=\"One,Two\"]",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [InlineNode::Macro(InlineMacro::IndexTerm(owner))] = parsed.inlines() else {
        return Err("index term".into());
    };
    let node = IndexTermRef::from(owner.as_ref());
    shared(node);
    assert!(!node.is_visible());
    assert_eq!(text(node.term())?, "Primary");
    assert!(
        node.secondary()
            .ok_or("present empty second level")?
            .is_empty()
    );
    assert_eq!(text(node.tertiary().ok_or("third level")?)?, "Tertiary");
    let Some(IndexTermRelationshipRef::SeeAlso { targets }) = node.relationship() else {
        return Err("see-also targets".into());
    };
    shared(targets);
    assert!(!targets.is_empty());
    assert_eq!(
        targets.iter().map(text).collect::<Result<Vec<_>, _>>()?,
        ["One", "Two"]
    );
    assert_eq!(node.relationship(), node.catalog().relationship());
    assert_eq!(node.secondary(), node.catalog().secondary());
    assert_eq!(node.tertiary(), node.catalog().tertiary());
    Ok(())
}

#[rstest::rstest]
#[case("((Visible))", true)]
#[case("(((Hidden)))", false)]
#[case("indexterm2:[Visible]", true)]
#[case("indexterm:[Hidden]", false)]
fn index_reads_cover_each_form(#[case] source: &str, #[case] visible: bool) -> Result<(), Error> {
    let parsed = parse_inline(source, &Options::default())?;
    let [InlineNode::Macro(InlineMacro::IndexTerm(owner))] = parsed.inlines() else {
        return Err("index term".into());
    };
    let node = IndexTermRef::from(owner.as_ref());
    assert_eq!(node.is_visible(), visible);
    assert!(node.secondary().is_none());
    assert!(node.tertiary().is_none());
    assert!(node.relationship().is_none());
    assert_eq!(node.term(), node.catalog().term());
    Ok(())
}

#[test]
fn index_catalog_reads_preserve_registration_time_labels() -> Result<(), Error> {
    let parsed = parse(
        ":term: Expanded\n\n[subs=\"specialcharacters,macros,attributes\"]\nindexterm2:[{term}]",
        &Options::default(),
    )?;
    let [acdc_parser::Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
        return Err("paragraph".into());
    };
    let [InlineNode::Macro(InlineMacro::IndexTerm(owner))] = paragraph.content.as_slice() else {
        return Err("index term".into());
    };
    let node = IndexTermRef::from(owner.as_ref());
    let catalog = node.catalog();
    shared(catalog);
    assert_eq!(text(node.term())?, "Expanded");
    assert_eq!(text(catalog.term())?, "{term}");
    Ok(())
}

#[test]
fn index_reads_preserve_empty_see_target() -> Result<(), Error> {
    let parsed = parse_inline("indexterm2:[Visible,see=\"\"]", &Options::default())?;
    let [InlineNode::Macro(InlineMacro::IndexTerm(owner))] = parsed.inlines() else {
        return Err("index term".into());
    };
    let Some(IndexTermRelationshipRef::See { target }) =
        IndexTermRef::from(owner.as_ref()).relationship()
    else {
        return Err("present see relationship".into());
    };
    assert!(target.is_empty());
    Ok(())
}
