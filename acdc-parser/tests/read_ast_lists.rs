use acdc_parser::{
    Anchor, Block, CalloutRefKind, CompatibilityMode, ContentAttribute, ListItemCheckedStatus,
    Location, Options, SafeMode,
    ast::{
        AttributeValueRef, BlockKind, CalloutListRef, ContentRef, DescriptionListItemRef,
        DescriptionListRef, InlineKind, ListItemRef, OrderedListRef, UnorderedListRef,
    },
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

fn text(content: ContentRef<'_>) -> Result<String, Error> {
    let mut result = String::new();
    for inline in content.inlines() {
        let kind = inline.kind();
        if let InlineKind::Plain(node) = kind {
            result.push_str(node.text());
        } else if let InlineKind::Bold(node) = kind {
            result.push_str(&text(node.content())?);
        } else if let InlineKind::Raw(node) = kind {
            result.push_str(node.text());
        } else {
            return Err("unexpected test content".into());
        }
    }
    Ok(result)
}

#[rstest::rstest]
fn unordered_items_keep_principal_blocks_markers_and_checklist_states(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        ".Tasks\n[#tasks,role=todo,options=custom]\n* [ ] *First*\n+\nAttached.\n+\n** [x] Nested\n* Plain\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::UnorderedList(list) =
        parsed.document_ref().blocks().next().ok_or("list")?.kind()
    else {
        return Err("unordered list".into());
    };
    assert_eq!(text(list.title().ok_or("title")?)?, "Tasks");
    assert_eq!(list.id().ok_or("id")?.as_str(), "tasks");
    assert_eq!(list.marker(), "*");
    assert!(list.roles().contains("todo"));
    assert!(list.options().contains("custom"));
    assert_eq!(
        list.attributes().get("title"),
        list.title().map(AttributeValueRef::Content)
    );
    let mut items = list.items();
    let first = items.next().ok_or("first")?;
    assert_eq!(text(first.principal())?, "First");
    assert_eq!(first.checked(), Some(ListItemCheckedStatus::Unchecked));
    assert_eq!(first.marker(), "*");
    let mut attached = first.blocks();
    let BlockKind::Paragraph(paragraph) = attached.next().ok_or("paragraph")?.kind() else {
        return Err("paragraph".into());
    };
    assert_eq!(text(paragraph.content())?, "Attached.");
    let BlockKind::UnorderedList(nested) = attached.next().ok_or("nested")?.kind() else {
        return Err("nested".into());
    };
    let child = nested.items().next().ok_or("child")?;
    assert_eq!(child.checked(), Some(ListItemCheckedStatus::Checked));
    assert_eq!(child.marker(), "**");
    assert!(child.level() > first.level());
    let second = items.next().ok_or("second")?;
    assert_eq!(text(second.principal())?, "Plain");
    assert_eq!(second.checked(), None);
    assert!(second.blocks().next().is_none());
    assert!(items.next().is_none());
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
fn ordered_lists_preserve_numbering_controls_without_rendering_numbers(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[loweralpha,start=3]\n. First\n. [x] Literal checkbox\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::OrderedList(list) = parsed.document_ref().blocks().next().ok_or("list")?.kind()
    else {
        return Err("ordered list".into());
    };
    assert_eq!(list.style(), Some("loweralpha"));
    assert_eq!(
        list.attributes().get("start"),
        Some(AttributeValueRef::Text("3"))
    );
    assert_eq!(list.marker(), ".");
    let mut items = list.items();
    assert_eq!(items.next().ok_or("first")?.marker(), ".");
    let second = items.next().ok_or("second")?;
    assert_eq!(second.checked(), None);
    assert_eq!(text(second.principal())?, "[x] Literal checkbox");
    Ok(())
}

#[rstest::rstest]
fn description_entries_keep_terms_principal_text_and_attached_blocks(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[horizontal]\n*First*::\nSecond:: Principal\n+\nAttached.\n\nThird:: Last\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::DescriptionList(list) =
        parsed.document_ref().blocks().next().ok_or("list")?.kind()
    else {
        return Err("description list".into());
    };
    assert_eq!(list.style(), Some("horizontal"));
    let mut items = list.items();
    let first = items.next().ok_or("first")?;
    assert_eq!(text(first.term())?, "First");
    assert!(first.principal().is_empty());
    assert!(first.blocks().next().is_none());
    let second = items.next().ok_or("second")?;
    assert_eq!(text(second.term())?, "Second");
    assert_eq!(text(second.principal())?, "Principal");
    assert_eq!(second.delimiter(), "::");
    let BlockKind::Paragraph(attached) = second.blocks().next().ok_or("attached")?.kind() else {
        return Err("paragraph".into());
    };
    assert_eq!(text(attached.content())?, "Attached.");
    assert_eq!(text(items.next().ok_or("third")?.term())?, "Third");
    assert!(items.next().is_none());
    Ok(())
}

#[rstest::rstest]
fn callout_items_preserve_marker_kinds_numbers_and_continuations(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "----\nfirst <.>\nsecond <2>\n----\n<.> First *explanation*\n+\nAttached.\n<2> Second\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let mut blocks = parsed.document_ref().blocks();
    let BlockKind::Listing(listing) = blocks.next().ok_or("listing")?.kind() else {
        return Err("listing".into());
    };
    let mut markers = listing.content().inlines().filter_map(|inline| {
        if let InlineKind::Callout(marker) = inline.kind() {
            Some(marker)
        } else {
            None
        }
    });
    let BlockKind::CalloutList(list) = blocks.next().ok_or("list")?.kind() else {
        return Err("callout list".into());
    };
    let mut items = list.items();
    let first = items.next().ok_or("first")?;
    let first_marker = markers.next().ok_or("first marker")?;
    assert_eq!(first.callout().number(), first_marker.number());
    assert_eq!(first.callout().kind(), CalloutRefKind::Auto);
    assert_eq!(first.callout().number(), 1);
    assert_eq!(text(first.principal())?, "First explanation");
    assert_eq!(first.blocks().count(), 1);
    assert!(first.callout().location().start().line() > first_marker.location().start().line());
    let second = items.next().ok_or("second")?;
    assert_eq!(
        second.callout().number(),
        markers.next().ok_or("second marker")?.number()
    );
    assert_eq!(second.callout().kind(), CalloutRefKind::Explicit);
    assert_eq!(second.callout().number(), 2);
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[test]
fn cloned_owners_preserve_empty_lists_items_and_anchor_content() -> Result<(), Error> {
    fn shared<T: Copy + Send + Sync>(_: T) {}
    let parsed = parse(
        "[title=\"\"]\n* Text\n\nTerm:: Value\n",
        &Options::default(),
    )?;
    let Some(Block::UnorderedList(list)) = parsed.document().blocks.first() else {
        return Err("list".into());
    };
    let mut list = list.clone();
    let mut item = list.items.first().ok_or("item")?.clone();
    item.principal.clear();
    item.blocks.clear();
    assert!(ListItemRef::from(&item).principal().is_empty());
    assert!(ListItemRef::from(&item).blocks().next().is_none());
    list.items.clear();
    let read = UnorderedListRef::from(&list);
    assert!(read.items().next().is_none());
    assert!(read.title().ok_or("empty title")?.is_empty());
    shared(read);
    let owner = parsed
        .document()
        .blocks
        .iter()
        .find_map(|b| {
            if let Block::DescriptionList(list) = b {
                list.items.first()
            } else {
                None
            }
        })
        .ok_or("description item")?;
    let mut owner = owner.clone();
    let mut anchor = Anchor::new("built-id", Location::default());
    anchor.xreflabel = Some(ContentAttribute::default());
    owner.anchors.push(anchor);
    let read = DescriptionListItemRef::from(&owner);
    let anchor = read.anchors().next().ok_or("anchor")?;
    assert_eq!(anchor.id(), "built-id");
    assert!(anchor.reftext().ok_or("empty reftext")?.is_empty());
    shared(read);
    std::thread::scope(|scope| {
        let anchors = read.anchors();
        scope.spawn(move || assert_eq!(anchors.count(), 1));
    });
    Ok(())
}

#[test]
fn included_description_delimiters_keep_unicode_source_coordinates() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let main = directory.path().join("main.adoc");
    let include = directory.path().join("terms.adoc");
    std::fs::write(&main, "include::terms.adoc[]\n")?;
    std::fs::write(&include, "λ:: Meaning\n")?;
    let parsed = parse_file(
        &main,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let BlockKind::DescriptionList(list) =
        parsed.document_ref().blocks().next().ok_or("list")?.kind()
    else {
        return Err("description list".into());
    };
    let item = list.items().next().ok_or("item")?;
    let location = parsed.resolve_location(item.delimiter_location().ok_or("delimiter")?);
    assert_eq!(location.file, Some(include));
    assert_eq!(
        (location.location.start.line, location.location.start.column),
        (1, 2)
    );
    assert_eq!(text(item.term())?, "λ");
    assert_eq!(text(item.principal())?, "Meaning");
    Ok(())
}

#[test]
fn list_owners_convert_to_the_same_public_handles() -> Result<(), Error> {
    let parsed = parse(
        ". Ordered\n\nTerm:: Value\n\n<1> Callout\n",
        &Options::default(),
    )?;
    assert_eq!(parsed.document().blocks.len(), 3);
    for block in &parsed.document().blocks {
        if let Block::OrderedList(list) = block {
            assert_eq!(OrderedListRef::from(list).items().count(), list.items.len());
        } else if let Block::DescriptionList(list) = block {
            assert_eq!(
                DescriptionListRef::from(list).items().count(),
                list.items.len()
            );
        } else if let Block::CalloutList(list) = block {
            assert_eq!(CalloutListRef::from(list).items().count(), list.items.len());
        } else {
            return Err("unexpected list form".into());
        }
    }
    Ok(())
}
