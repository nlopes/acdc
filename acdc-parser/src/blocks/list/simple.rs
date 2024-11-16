use std::collections::HashMap;

use acdc_core::{AttributeName, DocumentAttributes, Location, Position};
use pest::iterators::Pairs;
use tracing::instrument;

use crate::{
    model::{Block, BlockMetadata, ListItem, OrderedList, UnorderedList},
    Error, Rule,
};

impl Block {
    #[instrument(level = "trace")]
    pub(crate) fn parse_simple_list(
        pairs: Pairs<Rule>,
        title: Option<String>,
        metadata: BlockMetadata,
        attributes: HashMap<AttributeName, Option<String>>,
        parent_attributes: &mut DocumentAttributes,
    ) -> Result<Block, Error> {
        let mut location = Location::default();

        let mut items = Vec::new();
        let mut kind = "unordered";

        for pair in pairs {
            let span = pair.as_span();
            if location.start == location.end {
                location = Location {
                    start: Position {
                        line: span.start_pos().line_col().0,
                        column: span.start_pos().line_col().1,
                    },
                    end: Position {
                        line: span.end_pos().line_col().0,
                        column: span.end_pos().line_col().1,
                    },
                };
            }

            if span.start_pos().line_col().0 < location.start.line {
                location.start.line = span.start_pos().line_col().0;
            }
            if span.start_pos().line_col().1 < location.start.column {
                location.start.column = span.start_pos().line_col().1;
            }
            location.end.line = span.end_pos().line_col().0;
            location.end.column = span.end_pos().line_col().1;

            match pair.as_rule() {
                Rule::unordered_list_item => {
                    items.push(ListItem::parse(pair.into_inner(), parent_attributes)?);
                }
                Rule::ordered_list_item => {
                    kind = "ordered";
                    items.push(ListItem::parse(pair.into_inner(), parent_attributes)?);
                }
                unknown => unreachable!("{unknown:?}"),
            }
        }
        Ok(match kind {
            "ordered" => Block::OrderedList(OrderedList {
                title,
                metadata,
                attributes,
                items,
                location,
            }),
            _ => Block::UnorderedList(UnorderedList {
                title,
                metadata,
                attributes,
                items,
                location,
            }),
        })
    }
}
