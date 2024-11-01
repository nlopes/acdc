use std::collections::HashMap;

use pest::iterators::Pairs;
use tracing::instrument;

use crate::{
    model::{AttributeName, Audio, AudioSource, Block, BlockMetadata, Location},
    Rule,
};

impl Audio {
    #[instrument(level = "trace")]
    pub(crate) fn parse(
        pairs: Pairs<Rule>,
        metadata: &mut BlockMetadata,
        attributes: &mut HashMap<AttributeName, Option<String>>,
    ) -> Block {
        let mut source = AudioSource::Path(String::new());

        for pair in pairs {
            match pair.as_rule() {
                Rule::audio => {
                    for pair in pair.into_inner() {
                        match pair.as_rule() {
                            Rule::path => source = AudioSource::Path(pair.as_str().to_string()),
                            Rule::url => source = AudioSource::Url(pair.as_str().to_string()),
                            Rule::named_attribute => {
                                Block::parse_named_attribute(
                                    pair.into_inner(),
                                    attributes,
                                    metadata,
                                );
                            }
                            Rule::positional_attribute_value => {
                                tracing::warn!(
                                    name = pair.as_str(),
                                    "unexpected positional attribute in audio block"
                                );
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
        Block::Audio(Audio {
            location: Location::default(),
            title: None,
            source,
            metadata: metadata.clone(),
            attributes: attributes.clone(),
        })
    }
}