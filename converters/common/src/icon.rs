use acdc_parser::{AttributeValue, DocumentAttributes};

/// An `IconMode` represents the rendering mode for icons.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub enum IconMode {
    Font,
    Image,

    #[default]
    Text,
}
impl From<&DocumentAttributes> for IconMode {
    fn from(attrs: &DocumentAttributes) -> Self {
        if let Some(icons_value) = attrs.get("icons") {
            match icons_value {
                AttributeValue::String(s) if s == "image" => IconMode::Image,
                AttributeValue::Bool(true) => IconMode::Image,
                AttributeValue::String(s) if s == "font" => IconMode::Font,
                AttributeValue::String(_)
                | AttributeValue::Bool(_)
                | AttributeValue::Inlines(_) => {
                    tracing::warn!(
                        ?icons_value,
                        "Unknown icons attribute value, defaulting to Text mode"
                    );
                    IconMode::Text
                }
                AttributeValue::None => IconMode::Text,
            }
        } else {
            IconMode::Text
        }
    }
}
