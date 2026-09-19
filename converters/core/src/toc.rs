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

use acdc_parser::{AttributeValue, DocumentAttributes, SectionKind, TableOfContents, TocEntry};

use crate::section::{appendix_number_prefix, part_number_prefix, section_number_prefix};

/// Section-number settings used while rendering a table of contents.
#[derive(Debug, Clone, Copy)]
pub struct NumberingConfig<'a> {
    part_signifier: Option<&'a str>,
    chapter_signifier: Option<&'a str>,
    appendix_caption: Option<&'a str>,
}

impl<'a> NumberingConfig<'a> {
    /// Builds TOC numbering settings from document attributes and backend signifiers.
    #[must_use]
    pub fn new(
        attributes: &'a DocumentAttributes<'_>,
        part_signifier: Option<&'a str>,
        chapter_signifier: Option<&'a str>,
    ) -> Self {
        let appendix_caption = match attributes.get("appendix-caption") {
            Some(AttributeValue::String(caption)) => Some(caption.as_ref()),
            Some(AttributeValue::Bool(false)) => None,
            _ => Some("Appendix"),
        };
        Self {
            part_signifier,
            chapter_signifier,
            appendix_caption,
        }
    }
}

/// Returns whether the TOC contains a level-zero book part.
#[must_use]
pub fn has_real_parts(entries: &[TocEntry<'_>]) -> bool {
    entries
        .iter()
        .any(|entry| entry.level == 0 && entry.kind == SectionKind::Normal)
}

/// Returns the TOC level after applying book special-section placement rules.
#[must_use]
pub fn effective_level(entry: &TocEntry<'_>, has_real_parts: bool) -> u8 {
    if entry.level == 0 && entry.kind.is_special() && !has_real_parts {
        1
    } else {
        entry.level
    }
}

/// Computes the visible number prefix for each TOC entry.
#[must_use]
pub fn section_numbers(
    entries: &[TocEntry<'_>],
    config: &NumberingConfig<'_>,
) -> Vec<Option<String>> {
    entries
        .iter()
        .map(|entry| {
            let number = entry.number()?;
            Some(if entry.kind == SectionKind::Appendix {
                appendix_number_prefix(number, config.appendix_caption)
            } else if entry.level == 0 && entry.kind == SectionKind::Normal {
                part_number_prefix(number, config.part_signifier)
            } else if entry.level == 1 && entry.kind == SectionKind::Normal {
                section_number_prefix(number, config.chapter_signifier)
            } else {
                section_number_prefix(number, None)
            })
        })
        .collect()
}

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
        toc_macro: Option<&TableOfContents<'_>>,
        attributes: &DocumentAttributes<'_>,
    ) -> Self {
        let placement = attributes
            .get("toc")
            .map_or("none", |v| match v {
                // Empty string or Bool(true) means toc is enabled with auto placement
                AttributeValue::String(s) if s.is_empty() => "auto",
                AttributeValue::String(s) => s.as_ref(),
                AttributeValue::Bool(true) => "auto",
                // Bool(false), None, or unknown means toc is disabled
                AttributeValue::Bool(false) | AttributeValue::None | _ => "none",
            })
            .to_lowercase();

        let title = attributes
            .get("toc-title")
            .and_then(|v| match v {
                AttributeValue::String(s) => Some(s.as_ref()),
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
                AttributeValue::String(s) if !s.is_empty() => Some(s.clone().into_owned()),
                AttributeValue::String(_) | AttributeValue::Bool(_) | AttributeValue::None | _ => {
                    None
                }
            })
            .unwrap_or_else(|| match placement.as_ref() {
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
