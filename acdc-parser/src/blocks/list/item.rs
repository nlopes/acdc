use pest::iterators::Pairs;

use crate::{
    model::{DocumentAttributes, ListItem},
    Error, Rule,
};

impl ListItem {
    #[tracing::instrument(level = "trace")]
    pub(crate) fn parse(
        pairs: Pairs<Rule>,
        parent_attributes: &mut DocumentAttributes,
    ) -> Result<ListItem, Error> {
        let mut content = Vec::new();
        let mut level = 0;
        let mut checked = None;

        for pair in pairs {
            match pair.as_rule() {
                Rule::list_item => {
                    content.push(pair.as_str().trim().to_string());
                }
                Rule::unordered_level | Rule::ordered_level => {
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
            checked,
            content,
        })
    }
}
