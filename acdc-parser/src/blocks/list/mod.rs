mod description;
mod item;
mod simple;

use std::collections::HashMap;

use acdc_core::{DocumentAttributes, Location, Position};
use pest::iterators::Pairs;

use crate::{
    model::{Block, BlockMetadata, DescriptionList, UnorderedList},
    Error, Rule,
};

pub(crate) fn parse_list(
    pairs: Pairs<Rule>,
    parent_attributes: &mut DocumentAttributes,
) -> Result<Block, Error> {
    let mut title = None;
    let mut metadata = BlockMetadata::default();
    let mut attributes = HashMap::new();
    let mut style_found = false;
    let mut block = Block::UnorderedList(UnorderedList {
        title: None,
        metadata: metadata.clone(),
        attributes: attributes.clone(),
        items: Vec::new(),
        location: Location {
            start: Position { line: 0, column: 0 },
            end: Position { line: 0, column: 0 },
        },
    });

    for pair in pairs {
        match pair.as_rule() {
            Rule::list_title | Rule::blocktitle | Rule::title => {
                title = Some(pair.as_str().to_string());
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
                        attributes.insert(value, None);
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
                    attributes.clone(),
                    parent_attributes,
                )?;
            }
            Rule::EOI | Rule::comment => {}
            unknown => unreachable!("{unknown:?}"),
        }
    }
    Ok(block)
}
