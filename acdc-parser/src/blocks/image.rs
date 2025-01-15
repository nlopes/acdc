use std::collections::HashMap;

use pest::iterators::Pairs;
use tracing::instrument;

use crate::{
    Anchor, AttributeValue, Block, BlockMetadata, DocumentAttributes, ElementAttributes, Image,
    ImageSource, Location, Rule,
};

impl Image {
    #[instrument(level = "trace")]
    pub(crate) fn parse(
        pairs: Pairs<Rule>,
        metadata: &mut BlockMetadata,
        attributes: &mut ElementAttributes,
        parent_attributes: &mut DocumentAttributes,
    ) -> Block {
        let mut source = ImageSource::Path(String::new());
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
                Rule::anchor => {
                    tracing::error!("unexpected anchor in image block");
                    let anchor = Anchor::parse(pair.into_inner());
                    metadata.anchors.push(anchor);
                }
                Rule::image => {
                    Self::parse_inner(pair.into_inner(), metadata, attributes, &mut source);
                }
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        if let Some(anchor) = metadata.anchors.last() {
            metadata.id = Some(anchor.clone());
        }
        Block::Image(Self {
            location,
            title: Vec::new(),
            source,
            metadata: metadata.clone(),
        })
    }

    #[instrument(level = "trace")]
    fn parse_inner(
        pairs: Pairs<Rule>,
        metadata: &mut BlockMetadata,
        attributes: &mut ElementAttributes,
        source: &mut ImageSource,
    ) {
        let mut attribute_idx = 0;
        let mut attribute_mapping = HashMap::new();
        attribute_mapping.insert(0, "alt");
        attribute_mapping.insert(1, "width");
        attribute_mapping.insert(2, "height");

        for pair in pairs {
            match pair.as_rule() {
                Rule::path => *source = ImageSource::Path(pair.as_str().to_string()),
                Rule::url => *source = ImageSource::Url(pair.as_str().to_string()),
                Rule::named_attribute => {
                    Block::parse_named_attribute(pair.into_inner(), attributes, metadata);
                }
                Rule::positional_attribute_value => {
                    let value = pair.as_str().to_string();
                    if let Some(name) = attribute_mapping
                        .get(&attribute_idx)
                        .map(ToString::to_string)
                    {
                        attributes.insert(name, AttributeValue::String(value));
                    } else {
                        tracing::warn!(?value, "unexpected positional attribute in image block");
                    }
                    attribute_idx += 1;
                }
                Rule::open_sb | Rule::close_sb => {}
                unknown => unreachable!("{unknown:?}"),
            };
        }
    }
}
