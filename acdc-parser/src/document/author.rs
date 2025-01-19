use pest::iterators::Pairs;
use tracing::instrument;

use crate::{model::Author, Rule};

impl Author {
    #[instrument(level = "trace")]
    pub(crate) fn parse(pairs: Pairs<Rule>) -> Self {
        let mut first_name = String::new();
        let mut middle_name = None;
        let mut last_name = String::new();
        let mut initials = String::new();
        let mut email = None;

        for pair in pairs {
            match pair.as_rule() {
                Rule::author_first_name => {
                    first_name = pair.as_str().to_string();
                    initials.push(first_name.chars().next().unwrap_or_default());
                }
                Rule::author_middle_name => {
                    let text = pair.as_str();
                    middle_name = Some(text.to_string());
                    initials.push(text.chars().next().unwrap_or_default());
                }
                Rule::author_last_name => {
                    last_name = pair.as_str().to_string();
                    initials.push(last_name.chars().next().unwrap_or_default());
                }
                Rule::author_email => {
                    email = Some(pair.as_str().to_string()).map(|s| s.to_string());
                }
                unknown => unreachable!("{unknown:?}"),
            }
        }

        Self {
            first_name,
            middle_name,
            last_name,
            initials,
            email,
        }
    }
}
