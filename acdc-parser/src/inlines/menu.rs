use pest::iterators::Pairs;

use crate::{
    model::{Location, Menu},
    Rule,
};

impl Menu {
    pub(crate) fn parse_inline(pairs: Pairs<Rule>, location: Location) -> Self {
        let mut target = String::new();
        let mut items = Vec::new();
        for pair in pairs {
            match pair.as_rule() {
                Rule::path => target = pair.as_str().to_string(),
                Rule::menu_item => {
                    items.push(pair.as_str().to_string());
                }
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        Self {
            target,
            items,
            location,
        }
    }
}
