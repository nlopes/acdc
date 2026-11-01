//! List iterator copies keep independent positions over the same parsed items.

use acdc_parser::{
    CompatibilityMode, Options,
    ast::{BlockKind, CalloutListItemRef, DescriptionListItemRef, ListItemRef, LocationRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn check_positions<'doc>(
    mut items: impl Iterator<Item = LocationRef<'doc>> + Clone,
) -> Result<(), Error> {
    let mut beginning = items.clone();
    let first = items.next().ok_or("first item")?;
    let mut remaining = items.clone();
    let second = items.next().ok_or("second item")?;
    assert_ne!(first, second);
    assert!(items.next().is_none());
    assert_eq!(beginning.next(), Some(first));
    assert_eq!(beginning.next(), Some(second));
    assert_eq!(remaining.next(), Some(second));
    assert!(remaining.next().is_none());
    Ok(())
}

#[rstest::rstest]
fn list_item_iterators_clone_without_advancing_other_copies(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ". Ordered one\n. Ordered two\n\n//-\n\n* Unordered one\n* Unordered two\n\nFirst:: Description one.\nSecond:: Description two.\n\n[source]\n----\nfirst <1>\nsecond <2>\n----\n<1> Callout one\n<2> Callout two\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let mut kinds = Vec::new();
    for block in parsed.document_ref().blocks() {
        if let BlockKind::OrderedList(list) = block.kind() {
            check_positions(list.items().map(ListItemRef::location))?;
            kinds.push("ordered");
        } else if let BlockKind::UnorderedList(list) = block.kind() {
            check_positions(list.items().map(ListItemRef::location))?;
            kinds.push("unordered");
        } else if let BlockKind::DescriptionList(list) = block.kind() {
            check_positions(list.items().map(DescriptionListItemRef::location))?;
            kinds.push("description");
        } else if let BlockKind::CalloutList(list) = block.kind() {
            check_positions(list.items().map(CalloutListItemRef::location))?;
            kinds.push("callout");
        }
    }
    assert_eq!(kinds, ["ordered", "unordered", "description", "callout"]);
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}
