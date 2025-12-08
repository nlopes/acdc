use crate::AttributeValue;

#[derive(Debug)]
pub(crate) struct AttributeEntry<'a> {
    pub(crate) set: bool,
    pub(crate) key: &'a str,
    pub(crate) value: AttributeValue,
}

impl<'a> AttributeEntry<'a> {
    /// Handles unset attributes, boolean conversion, and string values
    pub(crate) fn new(key: &'a str, set: bool, value_opt: Option<&str>) -> Self {
        let value = if !set {
            // e.g: :!attr: or :attr!:
            AttributeValue::Bool(false)
        } else if let Some(v) = value_opt {
            match v.trim() {
                // Handle boolean strings
                "true" => AttributeValue::Bool(true),
                "false" => AttributeValue::Bool(false),
                _ => AttributeValue::String(v.to_string()),
            }
        } else {
            // No value means true (e.g: :toc:)
            AttributeValue::Bool(true)
        };
        Self { set, key, value }
    }
}
