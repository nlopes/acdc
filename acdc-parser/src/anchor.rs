use pest::iterators::Pairs;
use tracing::instrument;

use crate::{
    model::{Anchor, Position},
    Rule,
};

impl Anchor {
    #[instrument(level = "trace")]
    pub(crate) fn parse(pairs: Pairs<Rule>) -> Anchor {
        let mut anchor = Anchor::default();
        let len = pairs.clone().count();
        for (i, pair) in pairs.enumerate() {
            if i == 0 {
                anchor.location.start = Position {
                    line: pair.as_span().start_pos().line_col().0,
                    column: pair.as_span().start_pos().line_col().1,
                };
            }
            if i == len - 1 {
                anchor.location.end = Position {
                    line: pair.as_span().end_pos().line_col().0,
                    column: pair.as_span().end_pos().line_col().1,
                };
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
