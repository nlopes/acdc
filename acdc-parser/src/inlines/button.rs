use pest::iterators::Pairs;

use crate::{Button, Location, Rule};

impl Button {
    pub(crate) fn parse_inline(pairs: Pairs<Rule>, location: Location) -> Self {
        let mut label = String::new();
        for pair in pairs {
            match pair.as_rule() {
                Rule::label => label = pair.as_str().to_string(),
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        Self { label, location }
    }
}
