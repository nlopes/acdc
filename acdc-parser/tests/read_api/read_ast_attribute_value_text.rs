//! Exact attribute text does not render content or coerce other value kinds.

use acdc_parser::ast::AttributeValueRef;

type Error = Box<dyn std::error::Error>;

fn exact_text(value: AttributeValueRef<'_>) -> Option<&str> {
    value.as_str()
}

#[rstest::rstest]
fn exact_text_keeps_empty_quotes_spelling_and_unicode(
    #[values("", "plain", "false", "001", "\"quoted\"", " café 😀 ")] source: &str,
) -> Result<(), Error> {
    let owned = source.to_owned();
    let text = exact_text(AttributeValueRef::Text(&owned)).ok_or("text")?;
    assert_eq!(text, source);
    assert_eq!(text.as_ptr(), owned.as_ptr());
    Ok(())
}

#[test]
fn exact_text_is_available_in_constant_contexts() {
    const EMPTY: Option<&str> = AttributeValueRef::Text("").as_str();
    const BOOLEAN: Option<&str> = AttributeValueRef::Bool(false).as_str();
    assert_eq!(EMPTY, Some(""));
    assert_eq!(BOOLEAN, None);
}
