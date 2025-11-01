use acdc_parser::{AttributeValue, DocumentAttributes};

/// Configuration for the table of contents placement and options.
pub struct Config {
    pub placement: String,
    pub title: Option<String>,
    pub levels: u8,
}

impl From<&DocumentAttributes> for Config {
    fn from(attributes: &DocumentAttributes) -> Self {
        let placement = attributes
            .get("toc")
            .map_or("auto", |v| match v {
                AttributeValue::String(s) => s.as_str(),
                AttributeValue::Bool(true) => "auto",
                _ => "none",
            })
            .to_lowercase();
        let title = attributes
            .get("toc-title")
            .and_then(|v| match v {
                AttributeValue::String(s) => Some(s.as_str()),
                _ => None,
            })
            .map(String::from);

        let levels = attributes
            .get("toclevels")
            .and_then(|v| match v {
                AttributeValue::String(s) => s.parse::<u8>().ok(),
                _ => None,
            })
            .unwrap_or(2);

        Config {
            placement,
            title,
            levels,
        }
    }
}
