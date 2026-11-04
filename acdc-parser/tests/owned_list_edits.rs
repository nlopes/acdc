//! List edits transfer prepared items without rebuilding registrations or numbering.

type Error = Box<dyn std::error::Error>;

macro_rules! list_contract {
    ($module:ident, $owner:ident, $read:ident, $source:literal, $new:expr) => {
        mod $module {
            use super::Error;
            use acdc_parser::{
                Block, CompatibilityMode, Options, ParseResult, Title,
                ast::{ContentRef, $read},
                parse, $owner,
            };

            fn owner(parsed: &ParseResult) -> Result<$owner<'_>, Error> {
                for block in parsed.document().blocks() {
                    if let Block::$owner(owner) = Block::from(block) {
                        return Ok(owner);
                    }
                }
                Err("list".into())
            }

            #[rstest::rstest]
            fn prepared_values_round_trip_and_metadata_failures_are_atomic(
                #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)]
                mode: CompatibilityMode,
            ) -> Result<(), Error> {
                let parsed = parse(
                    $source,
                    &Options::builder().with_compatibility_mode(mode).build()?,
                )?;
                let mut list = owner(&parsed)?;
                let expected = list.clone();
                let before = serde_json::to_value(&list)?;
                let items = list.take_items();
                let metadata = list.take_metadata();
                let title = list.take_title();
                assert_eq!($read::from(&list).items().count(), 0);
                assert!($read::from(&list).title().is_none());
                list.set_items(items);
                list.set_metadata(metadata)?;
                list.set_title(title);
                assert_eq!(list, expected);
                assert_eq!(serde_json::to_value(&list)?, before);
                for name in [
                    "title",
                    "id",
                    "roles",
                    "opts",
                    "style",
                    "caption",
                    "reftext",
                    "subs",
                    "attribution",
                    "citetitle",
                ] {
                    let mut metadata = list.clone_metadata();
                    metadata.attributes.set(name.into(), "shadow".into());
                    assert!(list.set_metadata(metadata).is_err(), "{name}");
                    assert_eq!(list, expected, "{name}");
                }
                Ok(())
            }

            #[rstest::rstest]
            fn item_transfers_outlive_the_old_owner_and_clones_are_independent(
                #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)]
                mode: CompatibilityMode,
            ) -> Result<(), Error> {
                let parsed = parse(
                    $source,
                    &Options::builder().with_compatibility_mode(mode).build()?,
                )?;
                let original_document = serde_json::to_value(parsed.document())?;
                let expected = owner(&parsed)?;
                let (items, metadata, title) = {
                    let mut local = owner(&parsed)?;
                    let result = (
                        local.take_items(),
                        local.take_metadata(),
                        local.take_title(),
                    );
                    drop(local);
                    result
                };
                let mut rebuilt = $new;
                rebuilt.set_items(items);
                rebuilt.set_metadata(metadata)?;
                rebuilt.set_title(title);
                let expected_items = serde_json::to_value(&expected)?
                    .get("items")
                    .ok_or("items")?
                    .clone();
                assert_eq!(
                    serde_json::to_value(&rebuilt)?.get("items"),
                    Some(&expected_items)
                );
                assert_eq!(rebuilt.clone_metadata(), expected.clone_metadata());
                let mut copied = rebuilt.clone_items();
                assert_eq!(copied.len(), $read::from(&rebuilt).items().count());
                let last = copied.next_back().ok_or("last item")?;
                let count = $read::from(&rebuilt).items().count();
                rebuilt.push_item(last);
                assert_eq!($read::from(&rebuilt).items().count(), count + 1);
                assert_eq!(serde_json::to_value(parsed.document())?, original_document);
                let detached = rebuilt.clone_items();
                drop(rebuilt);
                assert_eq!(detached.len(), count + 1);
                Ok(())
            }

            #[test]
            fn constructors_keep_empty_titles_distinct_from_absence() -> Result<(), Error> {
                let mut list = $new;
                assert!($read::from(&list).title().is_none());
                assert_eq!($read::from(&list).items().count(), 0);
                list.set_title(Title::new(Vec::new()));
                assert!($read::from(&list).title().is_some_and(ContentRef::is_empty));
                assert!(list.take_title().is_present());
                assert!($read::from(&list).title().is_none());
                let list = list
                    .with_title(Title::new(Vec::new()))
                    .with_metadata(acdc_parser::BlockMetadata::new())?;
                assert!($read::from(&list).title().is_some_and(ContentRef::is_empty));
                Ok(())
            }
        }
    };
}

list_contract!(
    unordered,
    UnorderedList,
    UnorderedListRef,
    "[#list.note]\n.Title\n* [x] First *item* footnote:[Note].\n+\nAttached paragraph.\n\n* Second\n",
    acdc_parser::UnorderedList::new(Vec::new(), "*", acdc_parser::Location::default())
);
list_contract!(
    ordered,
    OrderedList,
    OrderedListRef,
    "[#list.note]\n.Title\n. First *item* footnote:[Note].\n+\nAttached paragraph.\n\n. Second\n",
    acdc_parser::OrderedList::new(Vec::new(), ".", acdc_parser::Location::default())
);
list_contract!(
    description,
    DescriptionList,
    DescriptionListRef,
    "[#list.note]\n.Title\n*Term*:: First footnote:[Note].\n+\nAttached paragraph.\n\nOther:: Second\n",
    acdc_parser::DescriptionList::new(Vec::new(), acdc_parser::Location::default())
);
list_contract!(
    callout,
    CalloutList,
    CalloutListRef,
    "----\nCode <1>\nCode <2>\n----\n\n[#list.note]\n.Title\n<1> First *item* footnote:[Note].\n+\nAttached paragraph.\n\n<2> Second\n",
    acdc_parser::CalloutList::new(Vec::new(), acdc_parser::Location::default())
);

#[test]
fn marker_replacements_do_not_rewrite_item_spelling_or_nesting() -> Result<(), Error> {
    use acdc_parser::{
        Block, Location, Options,
        ast::{OrderedListRef, UnorderedListRef},
        parse,
    };
    for source in ["* Parent\n** Child\n", ". First\n.. Nested\n"] {
        let parsed = parse(source, &Options::default())?;
        let mut found = 0;
        for block in parsed.document().blocks() {
            if let Block::UnorderedList(mut list) = Block::from(block) {
                found += 1;
                let items = serde_json::to_value(&list)?
                    .get("items")
                    .ok_or("items")?
                    .clone();
                assert_eq!(list.replace_marker("-"), "*");
                list.set_location(Location::default());
                assert_eq!(UnorderedListRef::from(&list).marker(), "-");
                assert_eq!(serde_json::to_value(&list)?.get("items"), Some(&items));
            } else if let Block::OrderedList(mut list) = Block::from(block) {
                found += 1;
                let items = serde_json::to_value(&list)?
                    .get("items")
                    .ok_or("items")?
                    .clone();
                assert_eq!(list.replace_marker("1."), ".");
                list.set_location(Location::default());
                assert_eq!(OrderedListRef::from(&list).marker(), "1.");
                assert_eq!(serde_json::to_value(&list)?.get("items"), Some(&items));
            }
        }
        assert_eq!(found, 1);
    }
    Ok(())
}
