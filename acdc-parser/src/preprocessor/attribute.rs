use crate::{
    AttributeValue, Error, Options,
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
pub(crate) fn parse_line(options: &mut Options<'_>, line: &str) -> Result<(), Error> {
    match attribute_parser::document_attribute(line) {
        Ok((unset, name, value)) => {
            let value = if unset {
                AttributeValue::Bool(false)
            } else {
                match value {
                    Some(value) => {
                        let value =
                            substitute(&value, HEADER, &options.document_attributes).into_owned();
                        if value.is_empty() {
                            AttributeValue::Bool(true)
                        } else {
                            AttributeValue::String(value.into())
                        }
                    }
                    None => AttributeValue::Bool(true),
                }
            };
            options.document_attributes.assign_document_value(
                name.into(),
                value,
                true,
                false,
                None,
            )?;
        }
        Err(e) => {
            tracing::warn!(?e, "Failed to parse attribute line");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> Options<'static> {
        Options::default().prepare_for_parse(crate::document_attribute::InputKind::String)
    }

    #[test]
    fn test_parse_simple_attribute() -> Result<(), Error> {
        let mut options = options();
        parse_line(&mut options, ":name: value")?;
        assert_eq!(options.document_attributes.text("name"), Some("value"));
        Ok(())
    }

    #[test]
    fn test_parse_unset_attribute() -> Result<(), Error> {
        let mut options = options();
        parse_line(&mut options, ":!name:")?;
        assert!(options.document_attributes.is_explicit("name"));
        assert_eq!(options.document_attributes.get("name"), None);
        Ok(())
    }

    #[test]
    fn test_parse_empty_value() -> Result<(), Error> {
        let mut options = options();
        parse_line(&mut options, ":name:")?;
        assert!(
            options
                .document_attributes
                .get("name")
                .is_some_and(crate::DocumentAttributeValue::is_presence)
        );
        Ok(())
    }

    #[test]
    fn test_parse_complex_name() -> Result<(), Error> {
        let mut options = options();
        parse_line(&mut options, ":complex-name_123: value")?;
        assert_eq!(
            options.document_attributes.text("complex-name_123"),
            Some("value")
        );
        Ok(())
    }

    #[test]
    fn test_definition_time_attribute_expansion() -> Result<(), Error> {
        // When bar is defined before foo, {bar} in foo's value should be expanded
        let mut options = options();
        parse_line(&mut options, ":bar: resolved-bar")?;
        parse_line(&mut options, ":foo: {bar}")?;

        // foo should have bar's value expanded at definition time
        assert_eq!(
            options.document_attributes.text("foo"),
            Some("resolved-bar")
        );
        Ok(())
    }

    #[test]
    fn test_undefined_attribute_kept_literal() -> Result<(), Error> {
        // When bar is NOT defined when foo is parsed, {bar} should stay literal
        let mut options = options();
        parse_line(&mut options, ":foo: {bar}")?;

        // foo should keep {bar} as literal since bar wasn't defined
        assert_eq!(options.document_attributes.text("foo"), Some("{bar}"));
        Ok(())
    }
}
