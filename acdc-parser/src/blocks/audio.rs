use pest::iterators::Pairs;
use tracing::instrument;

use crate::{
    Audio, AudioSource, Block, BlockMetadata, DocumentAttributes, ElementAttributes, Location, Rule,
};

impl Audio {
    #[instrument(level = "trace")]
    pub(crate) fn parse(
        pairs: Pairs<Rule>,
        metadata: &mut BlockMetadata,
        attributes: &mut ElementAttributes,
        parent_attributes: &mut DocumentAttributes,
    ) -> Block {
        let mut source = AudioSource::Path(String::new());
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
            location,
            title: Vec::new(),
            source,
            metadata: metadata.clone(),
        })
    }
}
