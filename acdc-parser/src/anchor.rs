use pest::iterators::Pairs;
use tracing::instrument;

use crate::{Rule, model::Anchor};

impl Anchor {
    #[instrument(level = "trace")]
    pub(crate) fn parse(pairs: Pairs<Rule>) -> Anchor {
        let mut anchor = Anchor::default();
        let len = pairs.clone().count();
        for (i, pair) in pairs.enumerate() {
            if i == 0 {
                anchor
                    .location
                    .set_start_from_pos(&pair.as_span().start_pos());
            }
            if i == len - 1 {
                anchor.location.set_end_from_pos(&pair.as_span().end_pos());
            }
            match pair.as_rule() {
                Rule::id => {
                    anchor.id = pair.as_str().to_string();
                }
                Rule::xreflabel => {
                    anchor.xreflabel = Some(pair.as_str().to_string());
                }
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        anchor
    }
}
