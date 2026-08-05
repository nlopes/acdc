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

use acdc_parser::{
    AttributeValue, DocumentAttributes, MAX_SECTION_LEVELS, MAX_TOC_LEVELS, SectionKind,
    TableOfContents, TocEntry,
};

use crate::section::{DEFAULT_SECTION_LEVEL, SpecialSectionTracker, to_upper_roman};

/// Section-number settings used while rendering a table of contents.
#[derive(Debug, Clone, Copy)]
pub struct NumberingConfig<'a> {
    sectnums_enabled: bool,
    sectnumlevels: u8,
    partnums_enabled: bool,
    part_signifier: Option<&'a str>,
    appendix_caption: Option<&'a str>,
}

impl<'a> NumberingConfig<'a> {
    /// Builds TOC numbering settings from document attributes.
    #[must_use]
    pub fn new(
        attributes: &'a DocumentAttributes<'_>,
        partnums_enabled: bool,
        part_signifier: Option<&'a str>,
    ) -> Self {
        let sectnums_enabled = attributes
            .get("sectnums")
            .or_else(|| attributes.get("numbered"))
            .is_some_and(|value| !matches!(value, AttributeValue::Bool(false)));
        let sectnumlevels = attributes
            .get_string("sectnumlevels")
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_SECTION_LEVEL)
            .min(MAX_SECTION_LEVELS);
        let appendix_caption = match attributes.get("appendix-caption") {
            Some(AttributeValue::String(caption)) => Some(caption.as_ref()),
            Some(AttributeValue::Bool(false)) => None,
            _ => Some("Appendix"),
        };
        Self {
            sectnums_enabled,
            sectnumlevels,
            partnums_enabled,
            part_signifier,
            appendix_caption,
        }
    }
}

/// Returns whether the TOC contains a level-zero book part.
#[must_use]
pub fn has_real_parts(entries: &[TocEntry<'_>]) -> bool {
    entries
        .iter()
        .any(|entry| entry.level == 0 && entry.kind != SectionKind::Appendix)
}

/// Returns the TOC level after applying book appendix placement rules.
#[must_use]
pub fn effective_level(entry: &TocEntry<'_>, has_real_parts: bool) -> u8 {
    if entry.level == 0 && entry.kind == SectionKind::Appendix && !has_real_parts {
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
    let has_appendix = entries
        .iter()
        .any(|entry| entry.kind == SectionKind::Appendix);
    if !config.sectnums_enabled && !config.partnums_enabled && !has_appendix {
        return vec![None; entries.len()];
    }

    let mut counters = [0u8; MAX_TOC_LEVELS as usize + 1];
    let mut part_counter = 0;
    let mut appendix_counter = 0;
    let mut appendix_letter = None;
    let mut numbers = Vec::with_capacity(entries.len());
    let special_sections = SpecialSectionTracker::new();

    for entry in entries {
        let level = entry.level;
        let numbered = special_sections.enter(level, entry.kind);

        if entry.kind == SectionKind::Appendix {
            counters.fill(0);
            let letter = char::from(b'A' + u8::try_from(appendix_counter).unwrap_or(25).min(25));
            appendix_counter += 1;
            appendix_letter = Some(letter);
            numbers.push(Some(match config.appendix_caption {
                Some(caption) => format!("{caption} {letter}: "),
                None => format!("{letter}. "),
            }));
            continue;
        }

        if level == 1 {
            appendix_letter = None;
        }

        if level == 0 {
            if config.partnums_enabled {
                part_counter += 1;
                let roman = to_upper_roman(part_counter);
                numbers.push(Some(match config.part_signifier {
                    Some(signifier) => format!("{signifier} {roman}: "),
                    None => format!("{roman}: "),
                }));
            } else {
                numbers.push(None);
            }
            continue;
        }

        if !numbered || level > MAX_TOC_LEVELS + 1 || !config.sectnums_enabled {
            numbers.push(None);
            continue;
        }

        let level_index = (level - 1) as usize;
        let Some(counter) = counters.get_mut(level_index) else {
            numbers.push(None);
            continue;
        };
        *counter += 1;
        for counter in counters.iter_mut().skip(level_index + 1) {
            *counter = 0;
        }

        if level > config.sectnumlevels {
            numbers.push(None);
            continue;
        }

        let number = if let Some(letter) = appendix_letter {
            counters.get(1..=level_index).map(|slice| {
                std::iter::once(letter.to_string())
                    .chain(slice.iter().map(ToString::to_string))
                    .collect::<Vec<_>>()
                    .join(".")
            })
        } else {
            counters.get(..=level_index).map(|slice| {
                slice
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(".")
            })
        };
        numbers.push(number.map(|number| format!("{number}. ")));
    }

    numbers
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
