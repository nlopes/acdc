use pest::iterators::Pairs;

use crate::{
    model::{AttributeName, AttributeValue, DocumentAttribute},
    Rule,
};

impl DocumentAttribute {
    pub(crate) fn parse(pairs: Pairs<Rule>) -> (AttributeName, AttributeValue) {
        let mut unset = false;
        let mut name = "";
        let mut value = None;

        for pair in pairs {
            match pair.as_rule() {
                Rule::attribute_name => {
                    name = pair.as_str();
                }
                Rule::unset => {
                    unset = true;
                }
                Rule::document_attribute_value => {
                    value = Some(pair.as_str().to_string());
                }
                unknown => {
                    tracing::warn!(?unknown, "unknown rule in header attribute");
                }
            }
        }
        if unset {
            (name.to_string(), AttributeValue::Bool(false))
        } else if let Some(value) = value {
            (name.to_string(), AttributeValue::String(value))
        } else {
            (name.to_string(), AttributeValue::Bool(true))
        }
    }
}
