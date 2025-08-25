use pest::iterators::Pairs;

use crate::{Autolink, Location, Rule, Source};

impl Autolink {
    pub(crate) fn parse_inline(pairs: Pairs<Rule>, location: Location) -> Self {
        let mut url = Source::Url(String::new());
        for pair in pairs {
            match pair.as_rule() {
                Rule::url => url = Source::Url(pair.as_str().to_string()),
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        Self { url, location }
    }
}
