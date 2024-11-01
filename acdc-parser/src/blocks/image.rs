use std::collections::HashMap;

use pest::iterators::Pairs;
use tracing::instrument;

use crate::{
    model::{Anchor, AttributeName, Block, BlockMetadata, Image, ImageSource, Location, Position},
    Rule,
};

impl Image {
    #[instrument(level = "trace")]
    pub(crate) fn parse(
        pairs: Pairs<Rule>,
        metadata: &mut BlockMetadata,
        attributes: &mut HashMap<AttributeName, Option<String>>,
    ) -> Block {
        let mut source = ImageSource::Path(String::new());
        let mut title = None;

        for pair in pairs {
            match pair.as_rule() {
                Rule::anchor => {
                    tracing::error!("unexpected anchor in image block");
                    let anchor = Anchor::parse(pair.into_inner());
                    metadata.anchors.push(anchor);
                }
                Rule::title => title = Some(pair.as_str().to_string()),
                Rule::image => {
                    Self::parse_inner(pair.into_inner(), attributes, &mut source, metadata);
                }
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        if let Some(anchor) = metadata.anchors.last() {
            metadata.id = Some(anchor.clone());
        }
        Block::Image(Self {
            location: Location {
                start: Position { line: 0, column: 0 },
                end: Position { line: 0, column: 0 },
            },
            title,
            source,
            metadata: metadata.clone(),
            attributes: attributes.clone(),
        })
    }

    #[instrument(level = "trace")]
    fn parse_inner(
        pairs: Pairs<Rule>,
        attributes: &mut HashMap<AttributeName, Option<String>>,
        source: &mut ImageSource,
        metadata: &mut BlockMetadata,
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
                        attributes.insert(name, Some(value));
                    } else {
                        tracing::warn!(?value, "unexpected positional attribute in image block");
                    }
                    attribute_idx += 1;
                }
                unknown => unreachable!("{unknown:?}"),
            };
        }
    }
}
