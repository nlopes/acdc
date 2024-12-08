use std::collections::HashMap;

use acdc_core::Location;
use pest::iterators::Pairs;

use crate::{
    model::{BlockMetadata, Image, ImageSource, OptionalAttributeValue},
    Rule,
};

impl Image {
    pub(crate) fn parse_inline(pairs: Pairs<Rule>, location: Location) -> Self {
        let mut metadata = BlockMetadata::default();
        let mut source = ImageSource::Path(String::new());
        let mut attributes = HashMap::new();
        for pair in pairs {
            match pair.as_rule() {
                Rule::path => source = ImageSource::Path(pair.as_str().to_string()),
                Rule::url => source = ImageSource::Url(pair.as_str().to_string()),
                Rule::named_attribute => {
                    super::parse_named_attribute(pair.into_inner(), &mut attributes);
                }
                Rule::positional_attribute_value => {
                    attributes.insert(pair.as_str().to_string(), OptionalAttributeValue(None));
                }
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        metadata.set_attributes(attributes);
        Self {
            metadata,
            title: Vec::new(), //attributes.remove("title").map(Option::unwrap_or_default),
            source,
            location,
        }
    }
}
