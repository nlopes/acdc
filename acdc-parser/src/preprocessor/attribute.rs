mod peg;

use peg::attribute_parser;

use crate::{
    Error, Options,
    document_attribute::{AttributeDeclaration, RawAttributeValue},
};

#[tracing::instrument(level = "trace", skip_all, fields(input_len = line.len()))]
pub(crate) fn parse_line(options: &mut Options<'_>, line: &str) -> Result<(), Error> {
    match attribute_parser::document_attribute(line) {
        Ok(AttributeDeclaration { name, value }) => {
            let value = value.resolve(&options.document_attributes).into_static();
            let value = if matches!(&value, RawAttributeValue::Resolved(value) if value.text() == Some(""))
            {
                RawAttributeValue::Set
            } else {
                value
            };
            options.document_attributes.assign_document_value(
                name.to_owned().into(),
                value,
                true,
                false,
                None,
            )?;
        }
        Err(_) => {
            tracing::warn!("failed to parse attribute line");
        }
    }
    Ok(())
}

pub(super) fn is_declaration(line: &str) -> bool {
    line.starts_with(':') && attribute_parser::document_attribute(line).is_ok()
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
    fn empty_expansion_becomes_presence_during_preprocessing() -> Result<(), Error> {
        let mut options = options();
        parse_line(&mut options, ":source:")?;
        parse_line(&mut options, ":expanded: {source}")?;
        assert!(
            options
                .document_attributes
                .get("expanded")
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
