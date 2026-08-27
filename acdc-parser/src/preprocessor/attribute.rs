use crate::{
    AttributeValue, Options,
    model::{HEADER, substitute},
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
pub(crate) fn parse_line(options: &mut Options<'_>, line: &str) {
    match attribute_parser::document_attribute(line) {
        Ok((unset, name, value)) => {
            if options.is_document_attribute_locked(&name, true) {
                return;
            }
            let attributes = &mut options.document_attributes;
            if unset {
                attributes.set(name.into(), AttributeValue::Bool(false));
            } else {
                let value = match value {
                    Some(v) => substitute(&v, HEADER, attributes).into_owned(),
                    None => String::new(),
                };
                attributes.set(name.into(), AttributeValue::String(value.into()));
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

    fn options() -> Options<'static> {
        Options::default().prepare_for_parse()
    }

    #[test]
    fn test_parse_simple_attribute() {
        let mut options = options();
        parse_line(&mut options, ":name: value");
        assert_eq!(
            options.document_attributes.get("name"),
            Some(&AttributeValue::String("value".into()))
        );
    }

    #[test]
    fn test_parse_unset_attribute() {
        let mut options = options();
        parse_line(&mut options, ":!name:");
        assert_eq!(
            options.document_attributes.get("name"),
            Some(&AttributeValue::Bool(false))
        );
    }

    #[test]
    fn test_parse_empty_value() {
        let mut options = options();
        parse_line(&mut options, ":name:");
        assert_eq!(
            options.document_attributes.get("name"),
            Some(&AttributeValue::String(std::borrow::Cow::Borrowed("")))
        );
    }

    #[test]
    fn test_parse_complex_name() {
        let mut options = options();
        parse_line(&mut options, ":complex-name_123: value");
        assert_eq!(
            options.document_attributes.get("complex-name_123"),
            Some(&AttributeValue::String("value".into()))
        );
    }

    #[test]
    fn test_definition_time_attribute_expansion() {
        // When bar is defined before foo, {bar} in foo's value should be expanded
        let mut options = options();
        parse_line(&mut options, ":bar: resolved-bar");
        parse_line(&mut options, ":foo: {bar}");

        // foo should have bar's value expanded at definition time
        assert_eq!(
            options.document_attributes.get("foo"),
            Some(&AttributeValue::String("resolved-bar".into()))
        );
    }

    #[test]
    fn test_undefined_attribute_kept_literal() {
        // When bar is NOT defined when foo is parsed, {bar} should stay literal
        let mut options = options();
        parse_line(&mut options, ":foo: {bar}");

        // foo should keep {bar} as literal since bar wasn't defined
        assert_eq!(
            options.document_attributes.get("foo"),
            Some(&AttributeValue::String("{bar}".into()))
        );
    }
}
