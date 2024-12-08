use acdc_core::{DocumentAttributes, Location, Position};
use pest::{iterators::Pairs, Parser as _};

use crate::{model::ListItem, Error, Rule};

impl ListItem {
    #[tracing::instrument(level = "trace")]
    pub(crate) fn parse(
        pairs: Pairs<Rule>,
        parent_attributes: &mut DocumentAttributes,
    ) -> Result<ListItem, Error> {
        let mut content = Vec::new();
        let mut level = 0;
        let mut marker = String::new();
        let mut checked = None;
        let mut location = Location::default();

        let len = pairs.clone().count();
        for (i, pair) in pairs.enumerate() {
            if i == 0 {
                location.start = Position {
                    line: pair.as_span().start_pos().line_col().0,
                    column: pair.as_span().start_pos().line_col().1,
                };
            }
            if i == len - 1 {
                location.end = Position {
                    line: pair.as_span().end_pos().line_col().0,
                    column: pair.as_span().end_pos().line_col().1,
                };
            }
            match pair.as_rule() {
                Rule::list_item => {
                    let current_pos = pair.as_span().start_pos();
                    let (current_start_line, current_start_column) =
                        (current_pos.line_col().0, current_pos.line_col().1);
                    match crate::InnerPestParser::parse(Rule::inlines, pair.as_str()) {
                        Ok(pairs) => {
                            for pair in pairs {
                                content.extend(crate::inlines::parse_inlines(
                                    pair,
                                    parent_attributes,
                                )?);
                            }
                            for inline in &mut content {
                                inline
                                    .shift_start_location(current_start_line, current_start_column);
                            }
                        }
                        Err(e) => {
                            tracing::error!("error parsing text: {e}");
                            return Err(Error::Parse(e.to_string()));
                        }
                    }
                }
                Rule::unordered_level | Rule::ordered_level => {
                    marker = pair.as_str().to_string();
                    level = u8::try_from(pair.as_str().chars().count())
                        .map_err(|e| Error::Parse(format!("error with list level depth: {e}")))?;
                }
                Rule::ordered_level_number => {
                    let number_string = pair.as_str();
                    level = number_string.parse::<u8>().map_err(|e| {
                        Error::Parse(format!(
                            "error with ordered level number {number_string}: {e}"
                        ))
                    })?;
                    // TODO(nlopes): implement ordered_level_number
                    //
                    // Do I need to? Does this make a difference? (Perhaps in providing errors
                    // to the user)
                }
                Rule::checklist_item_checked => checked = Some(true),
                Rule::checklist_item_unchecked => checked = Some(false),
                Rule::EOI | Rule::comment => {}
                unknown => unreachable!("{unknown:?}"),
            }
        }
        Ok(ListItem {
            level,
            marker,
            checked,
            content,
            location,
        })
    }
}
