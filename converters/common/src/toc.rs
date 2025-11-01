use acdc_parser::{AttributeValue, DocumentAttributes, TableOfContents};

/// Configuration for the table of contents placement and options.
pub struct Config {
    pub placement: String,
    pub title: Option<String>,
    pub levels: u8,
}

impl Config {
    /// Create a Config from document attributes and an optional TOC macro
    /// Block-level attributes from the toc macro take precedence over document attributes
    pub fn from_attributes(
        toc_macro: Option<&TableOfContents>,
        attributes: &DocumentAttributes,
    ) -> Self {
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

        // First check if toc macro has a levels attribute (block-level)
        let levels = toc_macro
            .and_then(|toc| toc.metadata.attributes.get("levels"))
            .and_then(|v| match v {
                AttributeValue::String(s) => s.parse::<u8>().ok(),
                _ => None,
            })
            .or_else(|| {
                // Fall back to document-level toclevels attribute
                attributes.get("toclevels").and_then(|v| match v {
                    AttributeValue::String(s) => s.parse::<u8>().ok(),
                    _ => None,
                })
            })
            .unwrap_or(2);

        Config {
            placement,
            title,
            levels,
        }
    }
}
