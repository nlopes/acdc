use std::collections::HashMap;

use pest::iterators::Pairs;
use tracing::instrument;

use crate::{
    model::{AttributeName, Block, BlockMetadata, Location, Video, VideoSource},
    Rule,
};

impl Video {
    #[instrument(level = "trace")]
    pub(crate) fn parse(
        pairs: Pairs<Rule>,
        metadata: &mut BlockMetadata,
        attributes: &mut HashMap<AttributeName, Option<String>>,
    ) -> Block {
        let mut sources = vec![];
        let mut attribute_idx = 0;

        for pair in pairs {
            match pair.as_rule() {
                Rule::video => {
                    for pair in pair.into_inner() {
                        match pair.as_rule() {
                            Rule::path => {
                                sources.push(VideoSource::Path(pair.as_str().to_string()));
                            }
                            Rule::url => sources.push(VideoSource::Url(pair.as_str().to_string())),
                            Rule::named_attribute => {
                                Block::parse_named_attribute(
                                    pair.into_inner(),
                                    attributes,
                                    metadata,
                                );
                            }
                            Rule::positional_attribute_value => {
                                let name = pair.as_str().to_string();
                                if attribute_idx == 0 {
                                    attributes.insert(name, None);
                                } else {
                                    tracing::warn!(
                                        ?name,
                                        "unexpected positional attribute in video block"
                                    );
                                }
                                attribute_idx += 1;
                            }
                            Rule::EOI | Rule::comment => {}
                            unknown => unreachable!("{unknown:?}"),
                        }
                    }
                }
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        Block::Video(Video {
            location: Location::default(),
            title: None,
            sources,
            metadata: metadata.clone(),
            attributes: attributes.clone(),
        })
    }
}
