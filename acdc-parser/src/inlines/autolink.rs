use acdc_core::Location;
use pest::iterators::Pairs;

use crate::{model::Autolink, Rule};

impl Autolink {
    pub(crate) fn parse_inline(pairs: Pairs<Rule>, location: Location) -> Self {
        let mut url = String::new();
        for pair in pairs {
            match pair.as_rule() {
                Rule::url => url = pair.as_str().to_string(),
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        Self { url, location }
    }
}
