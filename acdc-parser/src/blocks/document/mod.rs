mod attribute;
mod author;
mod header;
mod tree_builder;
mod validate;

use std::collections::HashMap;

use acdc_core::{Location, Position};
use pest::iterators::Pairs;
use tracing::instrument;

use crate::{
    blocks,
    model::{Document, Header},
    Error, Rule,
};

impl Document {
    #[instrument(level = "trace")]
    pub(crate) fn parse(pairs: Pairs<Rule>) -> Result<Self, Error> {
        let mut document_header = None;
        let mut attributes = HashMap::new();
        let mut blocks = Vec::new();
        let mut location = Location::default();

        for (i, pair) in pairs.enumerate() {
            if i == 0 {
                location.start = Position {
                    line: pair.as_span().start_pos().line_col().0,
                    column: pair.as_span().start_pos().line_col().1,
                };
            }
            location.end = Position {
                line: pair.as_span().end_pos().line_col().0,
                column: pair.as_span().end_pos().line_col().1 - 1,
            };
            match pair.as_rule() {
                Rule::document_header => {
                    document_header = Header::parse(pair.into_inner(), &mut attributes)?;
                }
                Rule::blocks => {
                    blocks.extend(blocks::parse(pair.into_inner(), &mut attributes)?);
                }
                Rule::comment | Rule::EOI => {}
                unknown => unimplemented!("{:?}", unknown),
            }
        }

        tree_builder::build_section_tree(&mut blocks)?;
        validate::section_block_level(&blocks, None)?;

        Ok(Self {
            name: "document".to_string(),
            r#type: "block".to_string(),
            header: document_header,
            attributes,
            blocks,
            location,
        })
    }
}
