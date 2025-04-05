use std::str::FromStr;

use pest::iterators::Pairs;

use crate::{
    model::{AttributeValue, ElementAttributes, Location, Url},
    InlineNode, Plain, Rule, Source,
};

impl Url {
    pub(crate) fn parse_inline(pairs: Pairs<Rule>, location: Location) -> Self {
        let mut text = None;
        let mut target = Source::from_str("").unwrap();
        let mut attributes = ElementAttributes::default();
        for pair in pairs {
            match pair.as_rule() {
                Rule::url => target = Source::from_str(pair.as_str()).unwrap(),
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
            text: if let Some(t) = text {
                vec![InlineNode::PlainText(Plain {
                    content: t,
                    location: location.clone(),
                })]
            } else {
                vec![]
            },
            target,
            attributes,
            location,
        }
    }
}
