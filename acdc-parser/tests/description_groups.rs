use acdc_parser::{
    CompatibilityMode, DescriptionListDefinition, DescriptionListItem, Document, Options,
    ast::{
        BlockKind, ContentRef, ContentRole, DescriptionListDefinitionRef, DescriptionListItemRef,
        InlineKind, StructuralRef,
    },
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("first::\nsecond:: body", 1, Some(false))]
#[case("first::\n\nsecond:: body", 1, Some(false))]
#[case("first::\n// comment\nsecond:: body", 1, Some(false))]
#[case("first::\nsecond::", 1, None)]
#[case(":blank:\n\nfirst:: {blank}\nsecond:: body", 2, Some(true))]
#[case("first:: pass:[]\nsecond:: body", 2, Some(false))]
#[case("first::\nsecond::\n\nbody", 1, Some(false))]
fn description_groups_preserve_missing_and_empty_definitions(
    #[case] source: &str,
    #[case] count: usize,
    #[case] empty: Option<bool>,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::DescriptionList(list) = parsed.document().blocks().next().ok_or("list")?.kind()
    else {
        return Err("description list".into());
    };
    assert_eq!(list.items().count(), count);
    let item = list.items().next().ok_or("item")?;
    assert_eq!(item.terms().count(), if count == 1 { 2 } else { 1 });
    assert_eq!(
        item.definition()
            .and_then(DescriptionListDefinitionRef::principal)
            .map(ContentRef::is_empty),
        empty
    );
    assert_eq!(parsed.document().blocks().count(), 1);
    let native = serde_json::to_value(parsed.document())?;
    let group = native.pointer("/blocks/0/items/0").ok_or("group")?;
    assert_eq!(group.get("definition").is_some(), empty.is_some());
    if empty == Some(true) {
        assert_eq!(
            group.pointer("/definition/principal"),
            Some(&serde_json::json!([]))
        );
    }
    Ok(())
}

#[rstest::rstest]
fn shared_definition_visits_and_registers_authored_content_once(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[[first]]first::\n[[second]]second:: Shared footnote:shared[One note] and ((one index term)).\n+\n====\nShared block refers to footnote:shared[].\n====\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let document = parsed.document();
    let BlockKind::DescriptionList(list) = document.blocks().next().ok_or("list")?.kind() else {
        return Err("description list".into());
    };
    assert_eq!(list.items().count(), 1);
    let item = list.items().next().ok_or("item")?;
    assert_eq!(item.terms().count(), 2);
    assert_eq!(item.definition().ok_or("definition")?.blocks().count(), 1);
    assert_eq!(
        document
            .descendants()
            .filter(|node| matches!(node, StructuralRef::DescriptionListDefinition(_)))
            .count(),
        1
    );
    assert_eq!(
        document
            .descendants()
            .filter(|node| matches!(node, StructuralRef::DescriptionListTerm(_)))
            .count(),
        2
    );
    assert_eq!(
        document
            .content_occurrences()
            .filter(|field| field.role() == ContentRole::DescriptionTerm)
            .count(),
        2
    );
    assert_eq!(document.footnotes().count(), 1);
    let notes = document
        .content_occurrences()
        .flat_map(|field| field.content().inlines())
        .filter_map(|inline| {
            if let InlineKind::Footnote(note) = inline.kind() {
                Some(note)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(notes.len(), 2);
    assert_eq!(
        notes.iter().filter(|note| note.content().is_some()).count(),
        1
    );
    assert!(notes.iter().all(|note| note.number() == 1));
    assert_eq!(
        document
            .content_occurrences()
            .flat_map(|field| field.content().inlines())
            .filter(|inline| matches!(inline.kind(), InlineKind::IndexTerm(_)))
            .count(),
        1
    );
    let before = Document::from(document);
    let mut copied = DescriptionListItem::from(item);
    let definition = copied.take_definition().ok_or("copied definition")?;
    assert!(DescriptionListItemRef::from(&copied).definition().is_none());
    copied.set_definition(Some(definition));
    assert_eq!(
        DescriptionListDefinition::from(
            DescriptionListItemRef::from(&copied)
                .definition()
                .ok_or("restored definition")?
        ),
        DescriptionListDefinition::from(item.definition().ok_or("original definition")?)
    );
    assert_eq!(Document::from(document), before);
    Ok(())
}

#[rstest::rstest]
fn rebuilt_shared_definition_keeps_one_catalog_and_repairs_atomically(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    use acdc_parser::{Block, DescriptionList, DescriptionListTerm, ast::DocumentRef};
    let parsed = parse(
        "[[first]]first::\n[[second]]second:: Shared footnote:shared[One note] and ((one index term)).\n+\nAttached footnote:shared[].\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::DescriptionList(list) = parsed.document().blocks().next().ok_or("list")?.kind()
    else {
        return Err("description list".into());
    };
    let group = list.items().next().ok_or("group")?;
    let item = DescriptionListItem::new(
        group.terms().map(DescriptionListTerm::from).collect(),
        group.definition().map(DescriptionListDefinition::from),
        group.location().into(),
    );
    let list = DescriptionList::new(vec![item], list.location().into());
    let mut document = Document::from(parsed.document());
    document.set_blocks([Block::DescriptionList(list.clone())]);
    document.rebuild_footnotes()?;
    document.rebuild_references_and_toc()?;
    let read = DocumentRef::from(&document);
    assert_eq!(read.footnotes().count(), 1);
    assert!(read.references().contains("first"));
    assert!(read.references().contains("second"));
    assert_eq!(
        read.content_occurrences()
            .flat_map(|field| field.content().inlines())
            .filter(|inline| matches!(inline.kind(), InlineKind::IndexTerm(_)))
            .count(),
        1
    );
    let mut groups = list.clone().take_items().collect::<Vec<_>>();
    let item = groups.first_mut().ok_or("group")?;
    let mut definition = item.take_definition().ok_or("definition")?;
    definition.push_block(Block::Section(acdc_parser::Section::new(
        acdc_parser::Title::new(Vec::new()),
        1,
        Vec::new(),
        acdc_parser::Location::default(),
    )));
    item.set_definition(Some(definition));
    let mut broken = list;
    broken.set_items(groups);
    document.set_blocks([Block::DescriptionList(broken)]);
    let before = serde_json::to_value(DocumentRef::from(&document))?;
    assert!(document.rebuild_references_and_toc().is_err());
    assert_eq!(serde_json::to_value(DocumentRef::from(&document))?, before);
    Ok(())
}

#[rstest::rstest]
fn grouped_terms_and_definition_retain_separate_source_spans(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "α::\nβ:: body  \n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::DescriptionList(list) = parsed.document().blocks().next().ok_or("list")?.kind()
    else {
        return Err("description list".into());
    };
    let item = list.items().next().ok_or("item")?;
    let spans = item
        .terms()
        .map(|term| {
            let location = acdc_parser::Location::from(term.location());
            (
                location.absolute_start,
                location.absolute_end,
                location.start.line,
                location.end.line,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(spans, [(0, 3, 1, 1), (5, 8, 2, 2)]);
    let definition = item.definition().ok_or("definition")?;
    let location = acdc_parser::Location::from(definition.location());
    assert_eq!((location.absolute_start, location.absolute_end), (10, 13));
    assert_eq!(item.location().end(), definition.location().end());
    Ok(())
}

#[rstest::rstest]
#[case("first::\nsecond::\n+\n====\nAttached.\n====\n")]
#[case("first::\nsecond::\n\n* Child\n")]
#[case("first::\nsecond::\nchild::: Nested\n")]
fn attached_only_definitions_own_their_blocks_once(
    #[case] source: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::DescriptionList(list) = parsed.document().blocks().next().ok_or("list")?.kind()
    else {
        return Err("description list".into());
    };
    assert_eq!(list.items().count(), 1);
    let item = list.items().next().ok_or("item")?;
    assert_eq!(item.terms().count(), 2);
    let definition = item.definition().ok_or("definition")?;
    assert!(definition.principal().is_none());
    assert_eq!(definition.blocks().count(), 1);
    assert_eq!(
        definition.location(),
        definition.blocks().next().ok_or("block")?.location()
    );
    Ok(())
}

#[rstest::rstest]
#[case("first::\n\n[role=outside]\nParagraph.", 2)]
#[case("first::\n\n----\nCode\n----", 2)]
#[case("first::\n////\nComment\n////\nsecond:: body", 3)]
fn block_boundaries_leave_pending_description_definitions_absent(
    #[case] source: &str,
    #[case] blocks: usize,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.document().blocks().count(), blocks);
    let BlockKind::DescriptionList(list) = parsed.document().blocks().next().ok_or("list")?.kind()
    else {
        return Err("description list".into());
    };
    let item = list.items().next().ok_or("item")?;
    assert_eq!(item.terms().count(), 1);
    assert!(item.definition().is_none());
    Ok(())
}
