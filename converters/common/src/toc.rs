use acdc_parser::{AttributeValue, DocumentAttributes};

/// Get the placement of the table of contents from document attributes.
/// Returns "auto", "preamble", "macro", or "none".
/// Defaults to "auto" if not specified or if set to true.
#[must_use]
pub fn get_placement_from_attributes(attributes: &DocumentAttributes) -> &str {
    attributes.get("toc").map_or("auto", |v| match v {
        AttributeValue::String(s) => s.as_str(),
        AttributeValue::Bool(true) => "auto",
        _ => "none",
    })
}
