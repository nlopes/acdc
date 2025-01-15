use std::collections::HashSet;

use pest::iterators::Pairs;

use crate::{Location, Pass, Rule, Substitution};

impl Pass {
    pub(crate) fn parse_inline(pairs: Pairs<Rule>, location: Location) -> Self {
        let mut text = None;
        let mut substitutions = HashSet::new();
        for pair in pairs {
            match pair.as_rule() {
                Rule::pass_inline_text => text = Some(pair.as_str().to_string()),
                Rule::substitution_value => {
                    substitutions.insert(Substitution::from(pair.as_str()));
                }
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        Self {
            text,
            substitutions,
            location,
        }
    }
}
