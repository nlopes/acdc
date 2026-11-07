use crate::{
    Anchor, Block, ContentAttribute, Location, Options,
    ast::{
        CalloutListRef, DescriptionListRef, DescriptionListTermRef, ListItemRef, OrderedListRef,
        UnorderedListRef,
    },
    parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn cloned_owners_preserve_empty_lists_items_and_anchor_content() -> Result<(), Error> {
    fn shared<T: Copy + Send + Sync>(_: T) {}
    let parsed = parse(
        "[title=\"\"]\n* Text\n\n[]\n\nTerm:: Value\n",
        &Options::default(),
    )?;
    let Some(Block::UnorderedList(list)) = parsed.document_storage().blocks.first() else {
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
        .document_storage()
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
    let mut owner = owner.terms.first().ok_or("term")?.clone();
    let mut anchor = Anchor::new("built-id", Location::default());
    anchor.xreflabel = Some(ContentAttribute::default());
    owner.anchors.push(anchor);
    let read = DescriptionListTermRef::from(&owner);
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
fn list_owners_convert_to_the_same_public_handles() -> Result<(), Error> {
    for source in [". Ordered\n", "Term:: Value\n", "<1> Callout\n"] {
        let parsed = parse(source, &Options::default())?;
        assert_eq!(parsed.document_storage().blocks.len(), 1);
        for block in &parsed.document_storage().blocks {
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
    }
    Ok(())
}
