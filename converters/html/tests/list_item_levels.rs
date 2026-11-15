//! HTML renders a list's items in order, whatever level each item retains.

use acdc_converters_core::{Converter as _, Options};
use acdc_converters_html::{Converter, HtmlVariant, RenderOptions};
use acdc_parser::{
    Block, Document, ListItem, Options as ParserOptions,
    ast::{BlockKind, DocumentRef, ListItemRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

const LEVELS: [u8; 3] = [1, 2, 0];

fn with_levels<'a>(items: impl Iterator<Item = ListItem<'a>>) -> Vec<ListItem<'a>> {
    items
        .zip(LEVELS)
        .map(|(mut item, level)| {
            item.set_level(level);
            item
        })
        .collect()
}

fn relevel(block: Block<'_>) -> Block<'_> {
    if let Block::UnorderedList(mut list) = block {
        let items = with_levels(list.take_items());
        list.set_items(items);
        Block::UnorderedList(list)
    } else if let Block::OrderedList(mut list) = block {
        let items = with_levels(list.take_items());
        list.set_items(items);
        Block::OrderedList(list)
    } else {
        block
    }
}

fn item_levels(document: DocumentRef<'_>) -> Vec<u8> {
    document
        .blocks()
        .flat_map(|block| {
            let levels: Vec<u8> = if let BlockKind::UnorderedList(list) = block.kind() {
                list.items().map(ListItemRef::level).collect()
            } else if let BlockKind::OrderedList(list) = block.kind() {
                list.items().map(ListItemRef::level).collect()
            } else {
                Vec::new()
            };
            levels
        })
        .collect()
}

#[rstest::rstest]
fn edited_item_levels_neither_nest_nor_drop_items(
    #[values("* One\n* Two\n* Three\n", ". One\n. Two\n. Three\n")] source: &str,
    #[values(HtmlVariant::Standard, HtmlVariant::Semantic)] variant: HtmlVariant,
) -> Result<(), Error> {
    let converter =
        Converter::new_with_variant(Options::default(), ParserOptions::builder(), variant)?;
    let parsed = parse(source, converter.parser_options())?;
    let mut document = Document::from(parsed.document());
    let blocks: Vec<_> = document.take_blocks().map(relevel).collect();
    document.set_blocks(blocks);
    assert_eq!(item_levels(DocumentRef::from(&document)), LEVELS);

    let options = RenderOptions {
        embedded: true,
        ..RenderOptions::default()
    };
    let expected = converter.convert_to_string(parsed.document(), &options)?;
    assert_eq!(
        converter.convert_to_string(DocumentRef::from(&document), &options)?,
        expected
    );
    for text in ["One", "Two", "Three"] {
        assert!(expected.contains(text), "{text}: {expected}");
    }
    Ok(())
}
