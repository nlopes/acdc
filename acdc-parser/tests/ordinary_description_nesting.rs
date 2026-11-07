use acdc_parser::{
    CompatibilityMode, Options,
    ast::{BlockKind, BlockRef, ContentRole, ElementIdRef, InlineKind, ListItemRef, StructuralRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn ordinary_items(block: BlockRef<'_>) -> Result<Vec<ListItemRef<'_>>, Error> {
    if let BlockKind::UnorderedList(list) = block.kind() {
        Ok(list.items().collect())
    } else if let BlockKind::OrderedList(list) = block.kind() {
        Ok(list.items().collect())
    } else {
        Err("expected ordinary list".into())
    }
}

#[rstest::rstest]
fn ordinary_items_nest_description_children(
    #[values("*", "-", ".", "1.")] marker: &str,
    #[values("", "\n", "+\n", "// comment\n", "wrapped principal\n")] gap: &str,
    #[values("::", ":::", ";;")] delimiter: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!(
        "{marker} Parent π\n{gap}[[term]]Term{delimiter} Definition footnote:[Once] and ((index)).\n{marker} Sibling:: ordinary text\n"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let document = parsed.document();
    assert_eq!(document.blocks().count(), 1);
    let items = ordinary_items(document.blocks().next().ok_or("list")?)?;
    let [first, second] = items.as_slice() else {
        return Err("expected two ordinary siblings".into());
    };
    let child = first.blocks().next().ok_or("nested list")?;
    let BlockKind::DescriptionList(list) = child.kind() else {
        return Err("expected description child".into());
    };
    assert_eq!(first.blocks().count(), 1);
    assert_eq!(list.items().count(), 1);
    let group = list.items().next().ok_or("group")?;
    assert_eq!(group.terms().count(), 1);
    let term = group.terms().next().ok_or("term")?;
    assert_eq!(term.delimiter(), delimiter);
    assert!(document.references().contains("term"));
    assert_eq!(document.footnotes().count(), 1);
    assert_eq!(
        document
            .content_occurrences()
            .flat_map(|field| field.content().inlines())
            .filter(|inline| matches!(inline.kind(), InlineKind::IndexTerm(_)))
            .count(),
        1
    );
    assert_eq!(
        document
            .content_occurrences()
            .filter(|field| field.role() == ContentRole::DescriptionTerm)
            .count(),
        1
    );
    assert_eq!(
        list.location().absolute_start(),
        source.find("[[term]]").ok_or("term source")?
    );
    assert_eq!(
        term.location().absolute_end(),
        source.find(delimiter).ok_or("delimiter source")? + delimiter.len() - 1
    );
    assert!(first.location().absolute_start() <= list.location().absolute_start());
    assert_eq!(
        first.location().absolute_end(),
        list.location().absolute_end()
    );
    assert!(list.location().absolute_end() < second.location().absolute_start());
    assert_eq!(second.blocks().count(), 0);
    Ok(())
}

#[rstest::rstest]
fn description_children_return_to_ordinary_ancestors(
    #[values("*", ".")] marker: &str,
    #[values(false, true)] cross_family: bool,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let child_marker = match (marker, cross_family) {
        ("*", false) => "**",
        ("*", true) => ".",
        (_, false) => "..",
        (_, true) => "*",
    };
    let source = format!(
        "{marker} Parent\nFirst::\nSecond:: Shared.\n{child_marker} Child\n{marker} Sibling:: ordinary text\n"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let items = ordinary_items(parsed.document().blocks().next().ok_or("list")?)?;
    let [first, second] = items.as_slice() else {
        return Err("expected two ordinary siblings".into());
    };
    let BlockKind::DescriptionList(list) = first.blocks().next().ok_or("description")?.kind()
    else {
        return Err("expected description child".into());
    };
    let group = list.items().next().ok_or("group")?;
    assert_eq!(group.terms().count(), 2);
    let children = ordinary_items(
        group
            .definition()
            .ok_or("definition")?
            .blocks()
            .next()
            .ok_or("child")?,
    )?;
    assert_eq!(children.len(), 1);
    assert!(first.location().absolute_end() < second.location().absolute_start());
    Ok(())
}

#[rstest::rstest]
fn nested_description_continuations_keep_the_existing_paragraph_policy(
    #[values("*", ".")] marker: &str,
    #[values("", "\n")] gap: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source =
        format!("{marker} Parent\nTerm:: Definition.\n{gap}+\nAttached.\n{marker} Sibling\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let items = ordinary_items(parsed.document().blocks().next().ok_or("list")?)?;
    assert_eq!(
        items.len(),
        if mode == CompatibilityMode::Asciidoctor {
            2
        } else {
            1
        }
    );
    let BlockKind::DescriptionList(list) = items
        .first()
        .ok_or("first ordinary item")?
        .blocks()
        .next()
        .ok_or("description")?
        .kind()
    else {
        return Err("expected description child".into());
    };
    let body = list
        .items()
        .next()
        .ok_or("group")?
        .definition()
        .ok_or("definition")?;
    assert_eq!(body.blocks().count(), usize::from(gap.is_empty()));
    assert_eq!(
        items.first().ok_or("first ordinary item")?.blocks().count(),
        if gap.is_empty() { 1 } else { 2 }
    );
    Ok(())
}

#[rstest::rstest]
fn description_child_metadata_and_separators_keep_their_owners(
    #[values("*", ".")] marker: &str,
    #[values("[#nested]\n", "\n[#nested]\n", "\n//\n")] gap: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("[#parent]\n{marker} Parent\n{gap}Term:: Definition.\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let document = parsed.document();
    let attached = !gap.starts_with('\n');
    assert_eq!(
        document.blocks().count(),
        if attached {
            1
        } else if gap.contains("//") {
            3
        } else {
            2
        }
    );
    let items = ordinary_items(document.blocks().next().ok_or("list")?)?;
    assert_eq!(
        items.first().ok_or("first ordinary item")?.blocks().count(),
        usize::from(attached)
    );
    let list = document
        .descendants()
        .find_map(|node| {
            if let StructuralRef::Block(block) = node
                && let BlockKind::DescriptionList(list) = block.kind()
            {
                Some(list)
            } else {
                None
            }
        })
        .ok_or("description")?;
    assert_eq!(
        list.id().map(ElementIdRef::as_str),
        if gap.contains("nested") {
            Some("nested")
        } else {
            None
        }
    );
    Ok(())
}

#[rstest::rstest]
fn ordinary_children_return_to_description_ancestors(
    #[values("*", ".")] marker: &str,
    #[values("::", ":::", ";;")] child_delimiter: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!(
        "Outer:: Body.\n{marker} Child\nInner{child_delimiter} Nested.\nFinal:: Final body.\n"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::DescriptionList(list) = parsed.document().blocks().next().ok_or("list")?.kind()
    else {
        return Err("description list".into());
    };
    assert_eq!(
        list.items().count(),
        if child_delimiter == "::" { 3 } else { 2 }
    );
    let item = list.items().next().ok_or("group")?;
    let children = ordinary_items(
        item.definition()
            .ok_or("body")?
            .blocks()
            .next()
            .ok_or("ordinary child")?,
    )?;
    assert_eq!(children.len(), 1);
    assert_eq!(
        children.first().ok_or("first child")?.blocks().count(),
        usize::from(child_delimiter != "::")
    );
    Ok(())
}

#[rstest::rstest]
fn description_scope_restores_after_delimited_content(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "* Parent\nTerm:: Outer.\n+\n--\n* Independent\nTerm:: Inner.\n* Inner sibling\n--\n* Outer sibling\n\n//\n\nTerm:: Top.\n* Top child\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let document = parsed.document();
    assert_eq!(
        ordinary_items(document.blocks().next().ok_or("outer list")?)?.len(),
        2
    );
    assert_eq!(
        document
            .descendants()
            .filter(|node| matches!(node, StructuralRef::ListItem(_)))
            .count(),
        5
    );
    assert_eq!(
        document
            .descendants()
            .filter(|node| matches!(node, StructuralRef::DescriptionListItem(_)))
            .count(),
        3
    );
    Ok(())
}

#[rstest::rstest]
fn ordinary_child_continuation_returns_to_description_parent(
    #[values("*", ".")] marker: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("Outer:: Body.\n{marker} Child\n+\nNext:: Next body.\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::DescriptionList(list) = parsed.document().blocks().next().ok_or("list")?.kind()
    else {
        return Err("description list".into());
    };
    assert_eq!(list.items().count(), 2);
    let children = ordinary_items(
        list.items()
            .next()
            .ok_or("group")?
            .definition()
            .ok_or("body")?
            .blocks()
            .next()
            .ok_or("child")?,
    )?;
    assert_eq!(children.len(), 1);
    assert_eq!(children.first().ok_or("first child")?.blocks().count(), 0);
    Ok(())
}

#[rstest::rstest]
fn description_syntax_does_not_change_the_existing_sibling_metadata_boundary(
    #[values("*", ".")] marker: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("{marker} Parent\n[#next]\n{marker} Sibling:: text\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.document().blocks().count(), 1);
    let items = ordinary_items(parsed.document().blocks().next().ok_or("list")?)?;
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|item| item.blocks().count() == 0));
    assert!(!parsed.document().references().contains("next"));
    Ok(())
}
