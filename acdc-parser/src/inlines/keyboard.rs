use pest::iterators::Pairs;

use crate::{Keyboard, Location, Rule};

impl Keyboard {
    pub(crate) fn parse_inline(pairs: Pairs<Rule>, location: Location) -> Self {
        let mut keys = Vec::new();
        for pair in pairs {
            match pair.as_rule() {
                Rule::key => keys.push(pair.as_str().to_string()),
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        Self { keys, location }
    }
}
