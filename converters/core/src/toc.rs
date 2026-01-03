//! Table of contents configuration.
//!
//! This module provides configuration for rendering the table of contents (TOC)
//! based on document attributes and TOC macro settings.
//!
//! # TOC Placement
//!
//! The `:toc:` attribute controls TOC placement:
//! - `auto` / empty - Render in preamble after abstract
//! - `left` / `right` - Render as sidebar
//! - `preamble` - Render at end of preamble
//! - `macro` - Render where `toc::[]` macro appears

use acdc_parser::{AttributeValue, DocumentAttributes, TableOfContents};

/// Configuration for the table of contents placement and options.
///
/// Created from document attributes using [`Config::from_attributes()`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Config {
    placement: String,
    title: Option<String>,
    levels: u8,
    toc_class: String,
}

impl Config {
    /// Create a Config from document attributes and an optional TOC macro.
    ///
    /// Block-level attributes from the toc macro take precedence over document attributes.
    #[must_use]
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
                AttributeValue::Bool(_) | AttributeValue::None | _ => "none",
            })
            .to_lowercase();

        let title = attributes
            .get("toc-title")
            .and_then(|v| match v {
                AttributeValue::String(s) => Some(s.as_str()),
                AttributeValue::Bool(_) | AttributeValue::None | _ => None,
            })
            .map(String::from);

        // First check if toc macro has a levels attribute (block-level)
        let levels = toc_macro
            .and_then(|toc| toc.metadata.attributes.get("levels"))
            .and_then(|v| match v {
                AttributeValue::String(s) => s.parse::<u8>().ok(),
                AttributeValue::Bool(_) | AttributeValue::None | _ => None,
            })
            .or_else(|| {
                // Fall back to document-level toclevels attribute
                attributes.get("toclevels").and_then(|v| match v {
                    AttributeValue::String(s) => s.parse::<u8>().ok(),
                    AttributeValue::Bool(_) | AttributeValue::None | _ => None,
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
                AttributeValue::String(_) | AttributeValue::Bool(_) | AttributeValue::None | _ => {
                    None
                }
            })
            .unwrap_or_else(|| match placement.as_str() {
                "left" | "right" | "top" | "bottom" => "toc2".to_string(),
                _ => "toc".to_string(),
            });

        Self {
            placement,
            title,
            levels,
            toc_class,
        }
    }

    /// Get the TOC placement position.
    ///
    /// Returns one of: "none", "auto", "left", "right", "preamble", "macro".
    #[must_use]
    pub fn placement(&self) -> &str {
        &self.placement
    }

    /// Get the TOC title, if set via `:toc-title:`.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Get the number of heading levels to include (default: 2).
    #[must_use]
    pub fn levels(&self) -> u8 {
        self.levels
    }

    /// Get the CSS class for the TOC container.
    ///
    /// Default is "toc2" for sidebar positions (left, right, top, bottom),
    /// "toc" for content positions (auto, preamble, macro).
    /// Can be overridden with `:toc-class:` attribute.
    #[must_use]
    pub fn toc_class(&self) -> &str {
        &self.toc_class
    }
}
