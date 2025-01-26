use pest::iterators::Pairs;

use crate::{
    model::{AttributeValue, ElementAttributes, Link, Location},
    Rule,
};

impl Link {
    pub(crate) fn parse_inline(pairs: Pairs<Rule>, location: Location) -> Self {
        let mut text = None;
        let mut target = String::new();
        let mut attributes = ElementAttributes::default();
        for pair in pairs {
            match pair.as_rule() {
                Rule::url | Rule::path => target = pair.as_str().to_string(),
                Rule::named_attribute => {
                    super::parse_named_attribute(pair.into_inner(), &mut attributes);
                }
                Rule::positional_attribute_value => {
                    attributes.insert(pair.as_str().to_string(), AttributeValue::None);
                }
                Rule::link_title => {
                    text = Some(pair.as_str().to_string());
                }
                Rule::EOI | Rule::comment | Rule::open_sb | Rule::close_sb => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        Self {
            text,
            target,
            attributes,
            location,
        }
    }
}
