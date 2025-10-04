use crate::{
    AttributeValue, DocumentAttributes,
    model::{HEADER, Substitute},
};

peg::parser! {
    grammar attribute_parser() for str {
        pub(crate) rule document_attribute() -> (bool, String, Option<String>)
            = ":" unset:unset() name:name() ":" { (true, name, None) }
            / ":" name:name() unset:unset() ":" { (true, name, None) }
            / ":" name:name() ":" whitespace()? value:value()? { (false, name, value) }

        rule unset() -> bool
            = "!" { true }

        rule name() -> String
            = n:$((['a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_']+)) { n.to_string() }

        rule value() -> String
            = v:$([^'\n']*) { v.to_string() }

        rule whitespace() = quiet!{[' ' | '\t']+}
    }
}

#[tracing::instrument(level = "trace")]
pub(crate) fn parse_line(attributes: &mut DocumentAttributes, line: &str) {
    match attribute_parser::document_attribute(line) {
        Ok((unset, name, value)) => {
            if unset {
                attributes.insert(name, AttributeValue::Bool(false));
            } else {
                let value = match value {
                    Some(v) => v.substitute(HEADER, attributes),
                    None => String::new(),
                };
                attributes.insert(name, AttributeValue::String(value));
            }
        }
        Err(e) => {
            tracing::warn!(?e, "Failed to parse attribute line");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_attribute() {
        let mut attributes = DocumentAttributes::default();
        parse_line(&mut attributes, ":name: value");
        assert_eq!(
            attributes.get("name"),
            Some(&AttributeValue::String("value".to_string()))
        );
    }

    #[test]
    fn test_parse_unset_attribute() {
        let mut attributes = DocumentAttributes::default();
        parse_line(&mut attributes, ":!name:");
        assert_eq!(attributes.get("name"), Some(&AttributeValue::Bool(false)));
    }

    #[test]
    fn test_parse_empty_value() {
        let mut attributes = DocumentAttributes::default();
        parse_line(&mut attributes, ":name:");
        assert_eq!(
            attributes.get("name"),
            Some(&AttributeValue::String(String::new()))
        );
    }

    #[test]
    fn test_parse_complex_name() {
        let mut attributes = DocumentAttributes::default();
        parse_line(&mut attributes, ":complex-name_123: value");
        assert_eq!(
            attributes.get("complex-name_123"),
            Some(&AttributeValue::String("value".to_string()))
        );
    }
}
