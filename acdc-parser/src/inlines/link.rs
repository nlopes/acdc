use std::{collections::HashMap, path::Path};

use acdc_core::Location;
use pest::iterators::Pairs;

use crate::{
    model::{Link, LinkTarget, OptionalAttributeValue},
    Rule,
};

impl Link {
    pub(crate) fn parse_inline(pairs: Pairs<Rule>, location: Location) -> Self {
        let mut target = LinkTarget::Url(String::new());
        let mut attributes = HashMap::new();
        for pair in pairs {
            dbg!(&pair);
            match pair.as_rule() {
                Rule::url => target = LinkTarget::Url(pair.as_str().to_string()),
                Rule::path => target = LinkTarget::Path(Path::new(pair.as_str()).to_path_buf()),
                Rule::role => {
                    attributes.insert(
                        "role".to_string(),
                        OptionalAttributeValue(Some(pair.as_str().to_string())),
                    );
                }
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
            target,
            attributes,
            location,
        }
    }
}
