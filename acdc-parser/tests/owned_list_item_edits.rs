//! Selected item copies and transfers preserve prepared content and source state.

use std::{alloc::System, hint::black_box};

use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

type Error = Box<dyn std::error::Error>;

macro_rules! item_contract {
    ($module:ident, $owner:ident, $read:ident, $kind:ident, $source:literal, $new:expr) => {
        mod $module {
            use super::Error;
            use acdc_parser::{
                CompatibilityMode, Location, Options, ParseResult,
                ast::{BlockKind, $read},
                parse, $owner,
            };

            fn owner(parsed: &ParseResult) -> Result<$owner<'_>, Error> {
                for block in parsed.document().blocks() {
                    if let BlockKind::$kind(list) = block.kind() {
                        let original = list.items().next().ok_or("item")?;
                        let copied = $owner::from(original);
                        let read = $read::from(&copied);
                        assert_eq!(read.principal(), original.principal());
                        assert_eq!(
                            read.blocks()
                                .map(acdc_parser::Block::from)
                                .collect::<Vec<_>>(),
                            original
                                .blocks()
                                .map(acdc_parser::Block::from)
                                .collect::<Vec<_>>()
                        );
                        assert_eq!(read.location(), original.location());
                        return Ok(copied);
                    }
                }
                Err("list".into())
            }

            #[rstest::rstest]
            fn selected_copies_and_content_round_trips_keep_all_prepared_values(
                #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)]
                mode: CompatibilityMode,
            ) -> Result<(), Error> {
                let parsed = parse(
                    $source,
                    &Options::builder().with_compatibility_mode(mode).build()?,
                )?;
                let mut item = owner(&parsed)?;
                let original = item.clone();
                let copy = $owner::from($read::from(&item));
                assert_eq!(copy, original);
                let principal = item.take_principal();
                let blocks = item.take_blocks();
                assert!($read::from(&item).principal().is_empty());
                assert_eq!($read::from(&item).blocks().count(), 0);
                item.set_principal(principal);
                item.set_blocks(blocks);
                assert_eq!(item, original);
                assert_eq!(
                    serde_json::to_value(&item)?,
                    serde_json::to_value(&original)?
                );
                Ok(())
            }

            #[rstest::rstest]
            fn principal_and_attached_blocks_outlive_the_old_owner(
                #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)]
                mode: CompatibilityMode,
            ) -> Result<(), Error> {
                let parsed = parse(
                    $source,
                    &Options::builder().with_compatibility_mode(mode).build()?,
                )?;
                let original_document = serde_json::to_value(parsed.document())?;
                let original = owner(&parsed)?;
                let (principal, blocks) = {
                    let mut local = owner(&parsed)?;
                    let result = (local.take_principal(), local.take_blocks());
                    drop(local);
                    result
                };
                let mut rebuilt = $new;
                rebuilt.set_principal(principal);
                rebuilt.set_blocks(blocks);
                let read = $read::from(&rebuilt);
                let original = $read::from(&original);
                assert_eq!(read.principal(), original.principal());
                assert_eq!(
                    read.blocks()
                        .map(acdc_parser::Block::from)
                        .collect::<Vec<_>>(),
                    original
                        .blocks()
                        .map(acdc_parser::Block::from)
                        .collect::<Vec<_>>()
                );
                rebuilt.push_block(acdc_parser::Block::Paragraph(acdc_parser::Paragraph::new(
                    Vec::new(),
                    Location::default(),
                )));
                rebuilt.set_location(Location::default());
                assert_eq!($read::from(&rebuilt).blocks().count(), 2);
                assert_eq!(serde_json::to_value(parsed.document())?, original_document);
                Ok(())
            }
        }
    };
}

item_contract!(
    ordinary,
    ListItem,
    ListItemRef,
    UnorderedList,
    "* [x] First *item* footnote:[Note].\n+\nAttached paragraph.\n",
    acdc_parser::ListItem::new(Vec::new(), 1, "*", acdc_parser::Location::default())
);
item_contract!(
    ordered,
    ListItem,
    ListItemRef,
    OrderedList,
    ". First *item* footnote:[Note].\n+\nAttached paragraph.\n",
    acdc_parser::ListItem::new(Vec::new(), 1, ".", acdc_parser::Location::default())
);
mod description {
    use super::Error;
    use acdc_parser::{
        Block, CompatibilityMode, DescriptionListDefinition, Location, Options, Paragraph,
        ParseResult,
        ast::{BlockKind, DescriptionListDefinitionRef},
        parse,
    };

    fn definition(parsed: &ParseResult) -> Result<DescriptionListDefinitionRef<'_>, Error> {
        let BlockKind::DescriptionList(list) =
            parsed.document().blocks().next().ok_or("list")?.kind()
        else {
            return Err("description list".into());
        };
        Ok(list
            .items()
            .next()
            .ok_or("item")?
            .definition()
            .ok_or("definition")?)
    }

    #[rstest::rstest]
    fn selected_copies_and_content_round_trips_keep_all_prepared_values(
        #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    ) -> Result<(), Error> {
        let parsed = parse(
            "*Term*:: Principal footnote:[Note].\n+\nAttached paragraph.\n",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let read = definition(&parsed)?;
        let mut owner = DescriptionListDefinition::from(read);
        let before = owner.clone();
        assert_eq!(
            DescriptionListDefinitionRef::from(&owner).principal(),
            read.principal()
        );
        assert_eq!(
            DescriptionListDefinitionRef::from(&owner).location(),
            read.location()
        );
        let principal = owner.take_principal();
        let blocks = owner.take_blocks();
        assert!(
            DescriptionListDefinitionRef::from(&owner)
                .principal()
                .is_none()
        );
        assert_eq!(
            DescriptionListDefinitionRef::from(&owner).blocks().count(),
            0
        );
        owner.set_principal(principal);
        owner.set_blocks(blocks);
        assert_eq!(owner, before);
        assert_eq!(
            serde_json::to_value(&owner)?,
            serde_json::to_value(&before)?
        );
        Ok(())
    }

    #[rstest::rstest]
    fn principal_and_attached_blocks_outlive_the_old_owner(
        #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    ) -> Result<(), Error> {
        let parsed = parse(
            "*Term*:: Principal footnote:[Note].\n+\nAttached paragraph.\n",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let original = definition(&parsed)?;
        let (principal, blocks) = {
            let mut owner = DescriptionListDefinition::from(original);
            (owner.take_principal(), owner.take_blocks())
        };
        let mut rebuilt = DescriptionListDefinition::new(None, Vec::new(), Location::default());
        rebuilt.set_principal(principal);
        rebuilt.set_blocks(blocks);
        let read = DescriptionListDefinitionRef::from(&rebuilt);
        assert_eq!(read.principal(), original.principal());
        assert_eq!(
            read.blocks().map(Block::from).collect::<Vec<_>>(),
            original.blocks().map(Block::from).collect::<Vec<_>>()
        );
        rebuilt.push_block(Block::Paragraph(Paragraph::new(
            Vec::new(),
            Location::default(),
        )));
        rebuilt.set_location(Location::default());
        assert_eq!(
            DescriptionListDefinitionRef::from(&rebuilt)
                .blocks()
                .count(),
            2
        );
        assert_eq!(serde_json::to_value(parsed.document())?, before);
        Ok(())
    }
}

item_contract!(
    callout,
    CalloutListItem,
    CalloutListItemRef,
    CalloutList,
    "----\nCode <.>\n----\n\n<.> First *explanation* footnote:[Note].\n+\nAttached paragraph.\n",
    acdc_parser::CalloutListItem::new(
        acdc_parser::CalloutRef::auto(1, acdc_parser::Location::default()),
        Vec::new(),
        acdc_parser::Location::default()
    )
);

#[test]
fn ordinary_item_scalars_and_checklist_presence_are_independent() {
    use acdc_parser::{ListItem, ListItemCheckedStatus, Location, ast::ListItemRef};
    let marker = String::from("**");
    let previous = {
        let mut item = ListItem::new(Vec::new(), 2, &marker, Location::default());
        item.set_checked(Some(ListItemCheckedStatus::Unchecked));
        assert_eq!(
            ListItemRef::from(&item).checked(),
            Some(ListItemCheckedStatus::Unchecked)
        );
        item.set_checked(Some(ListItemCheckedStatus::Checked));
        assert_eq!(
            ListItemRef::from(&item).checked(),
            Some(ListItemCheckedStatus::Checked)
        );
        item.set_checked(None);
        assert_eq!(ListItemRef::from(&item).checked(), None);
        item.set_level(7);
        assert_eq!(ListItemRef::from(&item).level(), 7);
        item.replace_marker("-")
    };
    assert_eq!(previous.as_ptr(), marker.as_ptr());
    assert_eq!(previous, "**");
}

#[test]
fn callout_replacement_keeps_the_supplied_kind_number_and_span() {
    use acdc_parser::{
        CalloutListItem, CalloutRef, CalloutRefKind, Location, ast::CalloutListItemRef,
    };
    let mut item = CalloutListItem::new(
        CalloutRef::auto(3, Location::default()),
        Vec::new(),
        Location::default(),
    );
    let old = item.replace_callout(CalloutRef::explicit(7, Location::default()));
    assert_eq!(
        acdc_parser::ast::CalloutRef::from(&old).kind(),
        CalloutRefKind::Auto
    );
    assert_eq!(acdc_parser::ast::CalloutRef::from(&old).number(), 3);
    let read = CalloutListItemRef::from(&item).callout();
    assert_eq!(read.kind(), CalloutRefKind::Explicit);
    assert_eq!(read.number(), 7);
    assert_eq!(Location::from(read.location()), Location::default());
}

#[rstest::rstest]
fn description_terms_anchors_and_delimiter_spans_transfer_independently(
    #[values(
        acdc_parser::CompatibilityMode::Asciidoctor,
        acdc_parser::CompatibilityMode::Draft
    )]
    mode: acdc_parser::CompatibilityMode,
) -> Result<(), Error> {
    use acdc_parser::{
        Anchor, DescriptionListTerm, Location, Options,
        ast::{BlockKind, DescriptionListTermRef},
        parse,
    };
    let parsed = parse(
        "[[entry]]\n*Term*:: Principal\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::DescriptionList(list) = parsed.document().blocks().next().ok_or("list")?.kind()
    else {
        return Err("list".into());
    };
    let original = list
        .items()
        .next()
        .ok_or("item")?
        .terms()
        .next()
        .ok_or("term")?;
    let mut item = DescriptionListTerm::from(original);
    assert_eq!(
        DescriptionListTermRef::from(&item).content(),
        original.content()
    );
    assert_eq!(
        DescriptionListTermRef::from(&item).delimiter(),
        original.delimiter()
    );
    let term = item.take_content();
    let anchors = item.take_anchors();
    assert!(DescriptionListTermRef::from(&item).content().is_empty());
    assert_eq!(DescriptionListTermRef::from(&item).anchors().count(), 0);
    item.set_content(term);
    item.set_anchors(anchors);
    assert_eq!(
        DescriptionListTermRef::from(&item).content(),
        original.content()
    );
    assert_eq!(
        DescriptionListTermRef::from(&item)
            .anchors()
            .map(|anchor| (
                anchor.id(),
                anchor.reftext(),
                Location::from(anchor.location())
            ))
            .collect::<Vec<_>>(),
        original
            .anchors()
            .map(|anchor| (
                anchor.id(),
                anchor.reftext(),
                Location::from(anchor.location())
            ))
            .collect::<Vec<_>>()
    );
    assert_eq!(item.replace_delimiter(";;"), "::");
    item.set_delimiter_location(None);
    item.push_anchor(Anchor::new("later", Location::default()));
    let read = DescriptionListTermRef::from(&item);
    assert_eq!(read.delimiter(), ";;");
    assert!(read.delimiter_location().is_none());
    assert_eq!(read.content(), original.content());
    assert_eq!(read.anchors().count(), original.anchors().count() + 1);
    Ok(())
}

#[test]
fn description_anchor_transfers_outlive_the_previous_item() -> Result<(), Error> {
    use acdc_parser::{Anchor, DescriptionListTerm, Location, ast::DescriptionListTermRef};

    let id = String::from("retained");
    let anchors = {
        let mut item = DescriptionListTerm::new(Vec::new(), "::", Location::default());
        item.push_anchor(Anchor::new(&id, Location::default()));
        item.take_anchors()
    };
    let mut rebuilt = DescriptionListTerm::new(Vec::new(), "::", Location::default());
    rebuilt.set_anchors(anchors);
    let read = DescriptionListTermRef::from(&rebuilt);
    let anchor = read.anchors().next().ok_or("anchor")?;
    assert_eq!(read.anchors().count(), 1);
    assert_eq!(anchor.id(), "retained");
    assert_eq!(anchor.id().as_ptr(), id.as_ptr());
    Ok(())
}

#[test]
fn selected_item_copy_cost_does_not_grow_with_sibling_count() -> Result<(), Error> {
    use acdc_parser::{
        CalloutListItem, DescriptionListItem, ListItem, Options, ast::BlockKind, parse,
    };
    fn cost<T>(copy: impl FnOnce() -> T) -> (usize, usize) {
        let region = Region::new(ALLOCATOR);
        let value = black_box(copy());
        let stats = region.change();
        drop(value);
        (stats.allocations, stats.bytes_allocated)
    }
    for source in ["* Item\n", ". Item\n", "Term:: Item\n", "<1> Item\n"] {
        let mut costs = Vec::new();
        for siblings in [1, 1000] {
            let input = source.repeat(siblings);
            let parsed = parse(&input, &Options::default())?;
            let block = parsed.document().blocks().next().ok_or("list")?;
            let allocations = if let BlockKind::UnorderedList(list) = block.kind() {
                assert_eq!(list.items().count(), siblings);
                let item = list.items().next().ok_or("item")?;
                cost(|| ListItem::from(item))
            } else if let BlockKind::OrderedList(list) = block.kind() {
                assert_eq!(list.items().count(), siblings);
                let item = list.items().next().ok_or("item")?;
                cost(|| ListItem::from(item))
            } else if let BlockKind::DescriptionList(list) = block.kind() {
                assert_eq!(list.items().count(), siblings);
                let item = list.items().next().ok_or("item")?;
                cost(|| DescriptionListItem::from(item))
            } else if let BlockKind::CalloutList(list) = block.kind() {
                assert_eq!(list.items().count(), siblings);
                let item = list.items().next().ok_or("item")?;
                cost(|| CalloutListItem::from(item))
            } else {
                return Err("list".into());
            };
            costs.push(allocations);
        }
        assert_eq!(costs.first(), costs.last(), "{source}");
    }
    Ok(())
}
