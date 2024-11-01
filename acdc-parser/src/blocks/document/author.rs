use pest::iterators::Pairs;
use tracing::instrument;

use crate::{model::Author, Rule};

impl Author {
    #[instrument(level = "trace")]
    pub(crate) fn parse(pairs: Pairs<Rule>) -> Self {
        let mut first_name = String::new();
        let mut middle_name = None;
        let mut last_name = String::new();
        let mut email = None;

        for pair in pairs {
            match pair.as_rule() {
                Rule::author_first_name => {
                    first_name = pair.as_str().to_string();
                }
                Rule::author_middle_name => middle_name = Some(pair.as_str().to_string()),
                Rule::author_last_name => {
                    last_name = pair.as_str().to_string();
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
            email,
        }
    }
}
