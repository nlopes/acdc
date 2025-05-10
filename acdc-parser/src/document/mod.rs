mod attribute;
mod author;
mod header;
mod tree_builder;
mod validate;

use pest::iterators::Pairs;
use tracing::instrument;

use crate::{Document, Error, Header, Location, Options, Rule, blocks};

impl Document {
    #[instrument(level = "trace")]
    pub(crate) fn parse(pairs: Pairs<Rule>, options: &Options) -> Result<Self, Error> {
        let mut document_header = None;
        let mut attributes = options.document_attributes.clone();
        let mut blocks = Vec::new();
        let mut location = Location::default();

        let len = pairs.clone().count();
        for (i, pair) in pairs.enumerate() {
            if i == 0 {
                location.set_start_from_pos(&pair.as_span().start_pos());
            }
            if i == len - 1 {
                location.set_end_from_pos(&pair.as_span().end_pos());
            }
            match pair.as_rule() {
                Rule::document_header => {
                    document_header = Header::parse(pair.into_inner(), options, &mut attributes)?;
                }
                Rule::blocks => {
                    blocks.extend(blocks::parse(
                        pair.into_inner(),
                        options,
                        None,
                        &mut attributes,
                    )?);
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
