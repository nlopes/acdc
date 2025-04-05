use pest::iterators::Pairs;
use tracing::instrument;

use crate::{
    AttributeValue, Block, BlockMetadata, DocumentAttributes, ElementAttributes, Location, Rule,
    Source, Video,
};

impl Video {
    #[instrument(level = "trace")]
    pub(crate) fn parse(
        pairs: Pairs<Rule>,
        metadata: &mut BlockMetadata,
        attributes: &mut ElementAttributes,
        parent_attributes: &mut DocumentAttributes,
    ) -> Block {
        let mut sources = vec![];
        let mut attribute_idx = 0;
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
                Rule::video => {
                    for pair in pair.into_inner() {
                        match pair.as_rule() {
                            Rule::path => {
                                sources.push(Source::Path(pair.as_str().to_string()));
                            }
                            Rule::url => sources.push(Source::Url(pair.as_str().to_string())),
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
                                    attributes.insert(name, AttributeValue::None);
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
            location,
            title: Vec::new(),
            sources,
            metadata: metadata.clone(),
        })
    }
}
