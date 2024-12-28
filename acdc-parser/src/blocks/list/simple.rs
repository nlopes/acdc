use std::collections::HashMap;

use pest::iterators::Pairs;
use tracing::instrument;

use crate::{
    AttributeName, Block, BlockMetadata, DocumentAttributes, Error, InlineNode, ListItem, Location,
    OptionalAttributeValue, OrderedList, Rule, UnorderedList,
};

impl Block {
    #[instrument(level = "trace")]
    pub(crate) fn parse_simple_list(
        pairs: Pairs<Rule>,
        title: Vec<InlineNode>,
        metadata: BlockMetadata,
        attributes: HashMap<AttributeName, OptionalAttributeValue>,
        parent_location: Option<&Location>,
        parent_attributes: &mut DocumentAttributes,
    ) -> Result<Block, Error> {
        let mut location = Location::default();

        let mut items = Vec::new();
        let mut kind = "unordered";

        let len = pairs.clone().count();
        for (i, pair) in pairs.enumerate() {
            if i == 0 {
                location.set_start_from_pos(&pair.as_span().start_pos());
            }
            if i == len - 1 {
                location.set_end_from_pos(&pair.as_span().end_pos());
            }
            match pair.as_rule() {
                Rule::unordered_list_item => {
                    items.push(ListItem::parse(
                        pair.into_inner(),
                        parent_location,
                        parent_attributes,
                    )?);
                }
                Rule::ordered_list_item => {
                    kind = "ordered";
                    items.push(ListItem::parse(
                        pair.into_inner(),
                        parent_location,
                        parent_attributes,
                    )?);
                }
                unknown => unreachable!("{unknown:?}"),
            }
        }

        // We need to clone the marker from the first item
        let marker = items[0].marker.clone();
        // let's shift the location by the parent location starting point
        location.shift(parent_location);
        Ok(match kind {
            "ordered" => Block::OrderedList(OrderedList {
                title,
                metadata,
                items,
                marker,
                location,
            }),
            _ => Block::UnorderedList(UnorderedList {
                title,
                metadata,
                items,
                marker,
                location,
            }),
        })
    }
}
