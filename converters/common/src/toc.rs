use acdc_parser::{AttributeValue, DocumentAttributes, TableOfContents};

/// Configuration for the table of contents placement and options.
pub struct Config {
    pub placement: String,
    pub title: Option<String>,
    pub levels: u8,
    /// CSS class for the TOC container div.
    /// Default is "toc2" for sidebar positions (left, right, top, bottom),
    /// "toc" for content positions (auto, preamble, macro).
    /// Can be overridden with `:toc-class:` attribute.
    pub toc_class: String,
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
            .map_or("none", |v| match v {
                // Empty string or Bool(true) means toc is enabled with auto placement
                AttributeValue::String(s) if s.is_empty() => "auto",
                AttributeValue::String(s) => s.as_str(),
                AttributeValue::Bool(true) => "auto",
                // Bool(false), None, or unknown means toc is disabled
                AttributeValue::Bool(_) | AttributeValue::None | AttributeValue::Inlines(_) | _ => {
                    "none"
                }
            })
            .to_lowercase();

        let title = attributes
            .get("toc-title")
            .and_then(|v| match v {
                AttributeValue::String(s) => Some(s.as_str()),
                AttributeValue::Bool(_) | AttributeValue::None | AttributeValue::Inlines(_) | _ => {
                    None
                }
            })
            .map(String::from);

        // First check if toc macro has a levels attribute (block-level)
        let levels = toc_macro
            .and_then(|toc| toc.metadata.attributes.get("levels"))
            .and_then(|v| match v {
                AttributeValue::String(s) => s.parse::<u8>().ok(),
                AttributeValue::Bool(_) | AttributeValue::None | AttributeValue::Inlines(_) | _ => {
                    None
                }
            })
            .or_else(|| {
                // Fall back to document-level toclevels attribute
                attributes.get("toclevels").and_then(|v| match v {
                    AttributeValue::String(s) => s.parse::<u8>().ok(),
                    AttributeValue::Bool(_)
                    | AttributeValue::None
                    | AttributeValue::Inlines(_)
                    | _ => None,
                })
            })
            .unwrap_or(2);

        // Compute toc-class: custom value, or "toc2" for sidebar positions, or "toc" otherwise
        // Sidebar positions (left, right, top, bottom) use "toc2" class for fixed positioning CSS
        // Content positions (auto, preamble, macro) use "toc" class for inline styling
        let toc_class = attributes
            .get("toc-class")
            .and_then(|v| match v {
                AttributeValue::String(s) if !s.is_empty() => Some(s.clone()),
                AttributeValue::String(_)
                | AttributeValue::Bool(_)
                | AttributeValue::None
                | AttributeValue::Inlines(_)
                | _ => None,
            })
            .unwrap_or_else(|| match placement.as_str() {
                "left" | "right" | "top" | "bottom" => "toc2".to_string(),
                _ => "toc".to_string(),
            });

        Config {
            placement,
            title,
            levels,
            toc_class,
        }
    }
}
