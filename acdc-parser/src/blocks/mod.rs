mod audio;
mod block;
mod delimited;
mod document;
mod image;
mod list;
mod paragraph;
mod section;
mod video;

use pest::iterators::Pairs;

use crate::{
    model::{Block, DocumentAttribute, Location, Position},
    Error, Rule,
};

pub(crate) fn parse(pairs: Pairs<Rule>) -> Result<Vec<Block>, Error> {
    if pairs.len() == 0 {
        return Ok(Vec::new());
    }
    if pairs.peek().is_none() {
        // TODO(nlopes): confirm if this is the correct behavior
        tracing::warn!(?pairs, "empty block");
        return Ok(vec![]);
    }
    let mut blocks = Vec::new();
    for pair in pairs {
        match pair.as_rule() {
            Rule::blocks => {
                blocks.extend(parse(pair.into_inner())?);
            }
            Rule::block => {
                blocks.push(Block::parse(pair.into_inner())?);
            }
            Rule::document_attribute => {
                let (name, value) = DocumentAttribute::parse(pair.clone().into_inner());
                let attribute = DocumentAttribute {
                    name,
                    value,
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
                };
                blocks.push(Block::DocumentAttribute(attribute));
            }
            Rule::EOI | Rule::comment => {}
            unknown => unreachable!("{unknown:?}"),
        }
    }
    Ok(blocks)
}