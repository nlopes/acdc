use pest::iterators::Pairs;

use crate::{AttributeName, AttributeValue, DocumentAttribute, DocumentAttributes, Rule};

impl DocumentAttribute {
    pub(crate) fn parse(
        pairs: Pairs<Rule>,
        parent_attributes: &mut DocumentAttributes,
    ) -> (AttributeName, AttributeValue) {
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
                    value = Some(AttributeValue::String(pair.as_str().to_string()));
                }
                unknown => {
                    tracing::warn!(?unknown, "unknown rule in header attribute");
                }
            }
        }
        let (name, value) = if unset {
            (name.to_string(), AttributeValue::Bool(false))
        } else if let Some(value) = value {
            (name.to_string(), value)
        } else {
            (name.to_string(), AttributeValue::Bool(true))
        };
        parent_attributes.insert(name.clone(), value.clone());
        (name, value)
    }
}
