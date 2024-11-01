use std::collections::HashMap;

use pest::iterators::Pairs;

use crate::{
    model::{Location, Url},
    Rule,
};

impl Url {
    pub(crate) fn parse_inline(pairs: Pairs<Rule>, location: Location) -> Self {
        let mut target = String::new();
        let mut attributes = HashMap::new();
        for pair in pairs {
            match pair.as_rule() {
                Rule::url => target = pair.as_str().to_string(),
                Rule::named_attribute => {
                    super::parse_named_attribute(pair.into_inner(), &mut attributes);
                }
                Rule::positional_attribute_value => {
                    attributes.insert(pair.as_str().to_string(), None);
                }
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        Self {
            target,
            attributes,
            location,
        }
    }
}
