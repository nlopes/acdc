use std::collections::HashMap;

use acdc_core::{DocumentAttributes, Location, Position};
use pest::{iterators::Pair, Parser as _};

use crate::{
    blocks,
    model::{Block, BlockMetadata, Section},
    Error, InnerPestParser, Rule,
};

impl Section {
    pub(crate) fn parse(
        pair: &Pair<Rule>,
        parent_attributes: &mut DocumentAttributes,
    ) -> Result<Block, Error> {
        let metadata = BlockMetadata::default();
        let attributes = HashMap::new();
        let mut title = String::new();
        let mut level = 0;
        let mut content = Vec::new();

        for inner_pair in pair.clone().into_inner() {
            match inner_pair.as_rule() {
                Rule::section_title => {
                    title = inner_pair.as_str().to_string();
                }
                Rule::section_level => {
                    level = u8::try_from(inner_pair.as_str().chars().count()).map_err(|e| {
                        Error::Parse(format!("error with section level depth: {e}"))
                    })? - 1;
                }
                Rule::section_content => {
                    let inner = inner_pair.clone().into_inner();
                    if inner.peek().is_none() {
                        let pairs = InnerPestParser::parse(Rule::document, inner_pair.as_str())
                            .map_err(|e| {
                                Error::Parse(format!("error parsing section content: {e}"))
                            })?;
                        content.extend(blocks::parse(pairs, parent_attributes)?);
                    } else {
                        for pair in inner {
                            content.extend(blocks::parse(pair.into_inner(), parent_attributes)?);
                        }
                    }
                }
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{:?}", unknown),
            }
        }

        Ok(Block::Section(Self {
            metadata,
            attributes,
            title,
            level,
            content,
            location: Location {
                start: Position {
                    line: pair.as_span().start_pos().line_col().0,
                    column: pair.as_span().start_pos().line_col().1,
                },
                end: Position {
                    line: pair.as_span().end_pos().line_col().0,
                    column: pair.as_span().end_pos().line_col().1,
                },
            },
        }))
    }
}
