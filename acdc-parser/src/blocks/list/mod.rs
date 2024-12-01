mod description;
mod item;
mod simple;

use std::collections::HashMap;

use acdc_core::{DocumentAttributes, Location, Position};
use pest::iterators::Pairs;

use crate::{
    inlines::parse_inlines,
    model::{Block, BlockMetadata, DescriptionList, OptionalAttributeValue, UnorderedList},
    Error, Rule,
};

use super::block::BlockExt;

pub(crate) fn parse_list(
    pairs: Pairs<Rule>,
    parent_attributes: &mut DocumentAttributes,
) -> Result<Block, Error> {
    let mut title = Vec::new();
    let mut metadata = BlockMetadata::default();
    let mut attributes = HashMap::new();
    let mut style_found = false;
    let mut block = Block::UnorderedList(UnorderedList {
        title: Vec::new(),
        metadata: metadata.clone(),
        items: Vec::new(),
        location: Location {
            start: Position { line: 0, column: 0 },
            end: Position { line: 0, column: 0 },
        },
    });

    for pair in pairs {
        match pair.as_rule() {
            Rule::list_title | Rule::blocktitle | Rule::title => {
                title = parse_inlines(pair, parent_attributes)?;
            }
            Rule::unordered_list | Rule::ordered_list => {
                block = Block::parse_simple_list(
                    pair.into_inner(),
                    title.clone(),
                    metadata.clone(),
                    attributes.clone(),
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
                        attributes.insert(value, OptionalAttributeValue(None));
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
