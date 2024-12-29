mod description;
mod item;
mod simple;

use pest::iterators::Pairs;

use crate::{
    inlines::parse_inlines, AttributeValue, Block, BlockMetadata, DescriptionList,
    DocumentAttributes, ElementAttributes, Error, Location, Rule, UnorderedList,
};

use super::block::BlockExt;

pub(crate) fn parse_list(
    pairs: Pairs<Rule>,
    parent_location: Option<&Location>,
    parent_attributes: &mut DocumentAttributes,
) -> Result<Block, Error> {
    let mut title = Vec::new();
    let mut metadata = BlockMetadata::default();
    let mut attributes = ElementAttributes::default();
    let mut style_found = false;
    let mut block = Block::UnorderedList(UnorderedList {
        title: Vec::new(),
        metadata: metadata.clone(),
        items: Vec::new(),
        marker: String::new(),
        location: Location::default(),
    });

    for pair in pairs {
        match pair.as_rule() {
            Rule::list_title | Rule::blocktitle | Rule::title => {
                title = parse_inlines(pair, parent_location, parent_attributes)?;
            }
            Rule::unordered_list | Rule::ordered_list => {
                block = Block::parse_simple_list(
                    pair.into_inner(),
                    title.clone(),
                    metadata.clone(),
                    attributes.clone(),
                    parent_location,
                    parent_attributes,
                )?;
            }
            Rule::named_attribute => {
                Block::parse_named_attribute(pair.into_inner(), &mut attributes, &mut metadata);
            }
            Rule::empty_style => {
                style_found = true;
            }
            Rule::positional_attribute_value => {
                let value = pair.as_str().to_string();
                if !value.is_empty() {
                    if metadata.style.is_none() && !style_found {
                        metadata.style = Some(value);
                    } else {
                        attributes.insert(value, AttributeValue::None);
                    }
                }
            }
            Rule::role => metadata.roles.push(pair.as_str().to_string()),
            Rule::option => metadata.options.push(pair.as_str().to_string()),
            Rule::description_list => {
                block = DescriptionList::parse(
                    pair.into_inner(),
                    title.clone(),
                    metadata.clone(),
                    parent_location,
                    parent_attributes,
                )?;
            }
            Rule::EOI | Rule::comment => {}
            unknown => unreachable!("{unknown:?}"),
        }
    }
    block.set_attributes(attributes);
    Ok(block)
}
