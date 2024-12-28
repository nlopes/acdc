use std::collections::HashMap;

use pest::iterators::Pairs;

use crate::{
    model::{Icon, Location, OptionalAttributeValue},
    Rule,
};

impl Icon {
    pub(crate) fn parse_inline(pairs: Pairs<Rule>, location: Location) -> Self {
        let mut path = String::new();
        let mut attributes = HashMap::new();
        for pair in pairs {
            match pair.as_rule() {
                Rule::path => path = pair.as_str().to_string(),
                Rule::named_attribute => {
                    super::parse_named_attribute(pair.into_inner(), &mut attributes);
                }
                Rule::positional_attribute_value => {
                    attributes.insert(pair.as_str().to_string(), OptionalAttributeValue(None));
                }
                Rule::EOI | Rule::comment | Rule::open_sb | Rule::close_sb => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        Self {
            target: path,
            attributes,
            location,
        }
    }
}
