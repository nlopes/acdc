mod audio;
mod block;
mod delimited;
mod document;
mod image;
mod list;
mod paragraph;
mod section;
mod table;
mod video;

use acdc_core::{DocumentAttributes, Location};
use pest::iterators::Pairs;

use crate::{
    model::{Block, DocumentAttribute},
    Error, Rule,
};

pub(crate) fn parse(
    pairs: Pairs<Rule>,
    parent_location: Option<&Location>,
    parent_attributes: &mut DocumentAttributes,
) -> Result<Vec<Block>, Error> {
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
                blocks.extend(parse(
                    pair.into_inner(),
                    parent_location,
                    parent_attributes,
                )?);
            }
            Rule::block => {
                blocks.push(Block::parse(
                    pair.into_inner(),
                    parent_location,
                    parent_attributes,
                )?);
            }
            Rule::document_attribute => {
                if parent_location.is_some() {
                    tracing::warn!("document attribute should account for parent_location");
                }
                let (name, value) =
                    DocumentAttribute::parse(pair.clone().into_inner(), parent_attributes);
                // TODO(nlopes): I don't think I need to store the document attributes as
                // a block anymore
                let attribute = DocumentAttribute {
                    name,
                    value,
                    location: Location::from_pair(&pair),
                };
                blocks.push(Block::DocumentAttribute(attribute));
            }
            Rule::EOI | Rule::comment => {}
            unknown => unreachable!("{unknown:?}"),
        }
    }
    Ok(blocks)
}
