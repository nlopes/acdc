use std::collections::HashMap;

use pest::iterators::Pairs;

use crate::{
    blocks,
    model::{
        Anchor, AttributeName, Block, BlockMetadata, DescriptionList, DescriptionListDescription,
        DescriptionListItem, Location, Position,
    },
    Error, Rule,
};

impl DescriptionList {
    pub(crate) fn parse(
        title: Option<String>,
        metadata: BlockMetadata,
        attributes: HashMap<AttributeName, Option<String>>,
        pairs: Pairs<Rule>,
    ) -> Result<Block, Error> {
        let mut location = Location {
            start: Position { line: 0, column: 0 },
            end: Position { line: 0, column: 0 },
        };

        let mut items = Vec::new();

        for pair in pairs {
            let location = Location {
                start: Position {
                    line: pair.as_span().start_pos().line_col().0,
                    column: pair.as_span().start_pos().line_col().1,
                },
                end: Position {
                    line: pair.as_span().end_pos().line_col().0,
                    column: pair.as_span().end_pos().line_col().1,
                },
            };
            let mut blocks = Vec::new();
            match pair.as_rule() {
                Rule::description_list_item => {
                    let mut anchors = Vec::new();
                    let mut term = String::new();
                    let mut delimiter = "";
                    for inner_pair in pair.clone().into_inner() {
                        let location = location.clone();
                        match inner_pair.as_rule() {
                            Rule::description_list_term => {
                                term = inner_pair.as_str().to_string();
                            }
                            Rule::description_list_term_anchor => {
                                anchors.push(Anchor::parse(inner_pair.into_inner()));
                            }
                            Rule::description_list_delimiter => {
                                delimiter = inner_pair.as_str();
                            }
                            Rule::blocks => {
                                let description = blocks::parse(inner_pair.into_inner())?;
                                items.push(DescriptionListItem {
                                    anchors: anchors.clone(),
                                    term: term.to_string(),
                                    delimiter: delimiter.to_string(),
                                    description: DescriptionListDescription::Blocks(description),
                                    location,
                                });
                            }
                            Rule::description_list_inline => {
                                let description = inner_pair.as_str();
                                items.push(DescriptionListItem {
                                    anchors: anchors.clone(),
                                    term: term.to_string(),
                                    delimiter: delimiter.to_string(),
                                    description: DescriptionListDescription::Inline(
                                        description.to_string(),
                                    ),
                                    location,
                                });
                            }
                            Rule::EOI | Rule::comment => {}
                            _ => {
                                // If we get here, it means we have a block that is not a
                                // description list
                                blocks.push(Block::parse(inner_pair.into_inner())?);
                            }
                        }
                    }
                    if !blocks.is_empty() {
                        items.push(DescriptionListItem {
                            anchors,
                            term: term.to_string(),
                            delimiter: delimiter.to_string(),
                            description: DescriptionListDescription::Blocks(blocks),
                            location,
                        });
                    }
                }
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }

        location.start = items
            .first()
            .map_or(location.start, |item| item.location.start.clone());
        location.end = items
            .last()
            .map_or(location.end, |item| item.location.end.clone());

        Ok(Block::DescriptionList(Self {
            title,
            metadata,
            attributes,
            items,
            location,
        }))
    }
}
