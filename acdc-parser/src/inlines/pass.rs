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

    pub(crate) fn parse_inline_single_double_or_triple(
        pairs: Pairs<Rule>,
        location: Location,
    ) -> Pass {
        let mut text = None;
        let mut substitutions = HashSet::new();
        substitutions.insert(Substitution::SpecialChars);
        for pair in pairs {
            match pair.as_rule() {
                Rule::single_double_passthrough | Rule::triple_passthrough => {
                    let pair = pair.into_inner().next().unwrap_or_else(|| {
                        unreachable!("single_double_passthrough or triple_passthrough should have a pair inside")
                    });
                    text = Some(pair.as_str().to_string());
                }
                Rule::substitution_value => {
                    substitutions.insert(Substitution::from(pair.as_str()));
                }
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        Pass {
            text,
            substitutions,
            location,
        }
    }
}
