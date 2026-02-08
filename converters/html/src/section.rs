use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use acdc_converters_core::visitor::WritableVisitor;
use acdc_parser::{
    AttributeValue, DiscreteHeader, DocumentAttributes, MAX_SECTION_LEVELS, Section,
    UNNUMBERED_SECTION_STYLES,
};

use crate::{Error, HtmlVariant, Processor};

pub(crate) const DEFAULT_SECTION_LEVEL: u8 = 3;

/// Tracks section numbers for `:sectnums:` attribute support.
/// Maintains hierarchical counters (e.g., "1.", "1.1.", "1.1.1.").
#[derive(Clone, Debug)]
pub(crate) struct SectionNumberTracker {
    /// Counters for each level (index 0 = level 1, etc.)
    counters: Rc<RefCell<[usize; MAX_SECTION_LEVELS as usize + 1]>>,
    /// Whether section numbering is enabled
    enabled: Rc<Cell<bool>>,
    /// Maximum level to number (from `:sectnumlevels:`, default 3)
    max_level: u8,
}

impl SectionNumberTracker {
    /// Create a new section number tracker.
    pub(crate) fn new(document_attributes: &DocumentAttributes) -> Self {
        // sectnums is enabled if the attribute exists and is not set to false
        // Also check :numbered: as a deprecated alias for :sectnums:
        let enabled = document_attributes
            .get("sectnums")
            .or_else(|| document_attributes.get("numbered"))
            .is_some_and(|v| !matches!(v, AttributeValue::Bool(false)));
        // Clamp to valid range: 0-5 (0 effectively disables numbering)
        let max_level = document_attributes
            .get_string("sectnumlevels")
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_SECTION_LEVEL)
            .min(MAX_SECTION_LEVELS);

        Self {
            counters: Rc::new(RefCell::new([0; MAX_SECTION_LEVELS as usize + 1])),
            enabled: Rc::new(Cell::new(enabled)),
            max_level,
        }
    }

    /// Enter a section and return its number if numbering is enabled.
    /// Returns None if numbering is disabled or section is beyond max level.
    #[must_use]
    pub(crate) fn enter_section(&self, level: u8) -> Option<String> {
        if !self.enabled.get() || level == 0 || level > self.max_level {
            return None;
        }

        let level_idx = (level - 1) as usize;
        let mut counters = self.counters.borrow_mut();

        // Safe indexing - level is validated to be 1..=max_level (max MAX_SECTION_LEVELS + 1)
        // and counters is [usize; MAX_SECTION_LEVELS + 1], so level_idx is always in bounds
        let counter = counters.get_mut(level_idx)?;
        *counter += 1;

        // Reset all deeper levels
        for c in counters.iter_mut().skip(level_idx + 1) {
            *c = 0;
        }

        // Build the number string (e.g., "1.2.3.")
        let number: String = counters
            .get(..=level_idx)?
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(".");

        Some(format!("{number}. "))
    }
}

/// Visit a section using the visitor pattern
///
/// Renders the section header, walks nested blocks, then renders footer.
/// For sections with `[index]` style, renders a populated index catalog
/// only if it's the last section in the document.
pub(crate) fn visit_section<V: WritableVisitor<Error = Error>>(
    section: &Section,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), Error> {
    // Check if this is an index section
    let is_index_section = section
        .metadata
        .style
        .as_ref()
        .is_some_and(|s| s == "index");

    // Index sections are only rendered if they're the last section
    if is_index_section && !processor.has_valid_index_section() {
        // Skip rendering entirely - not even the title
        return Ok(());
    }

    render_section_header(section, visitor, processor)?;

    if is_index_section {
        // Render the collected index catalog
        crate::index::render(section, visitor, processor)?;
    } else {
        // Normal section: render nested blocks
        for nested_block in &section.content {
            visitor.visit_block(nested_block)?;
        }
    }

    render_section_footer(section, visitor, processor)?;
    Ok(())
}

/// Render the section header (opening tags and title)
///
/// Call this before walking the section's nested blocks.
fn render_section_header<V: WritableVisitor<Error = Error>>(
    section: &Section,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), Error> {
    let level = section.level + 1; // Level 1 = h2
    let id = Section::generate_id(&section.metadata, &section.title);

    let mut w = visitor.writer_mut();

    if processor.variant() == HtmlVariant::Semantic {
        writeln!(w, "<section class=\"doc-section level-{}\">", section.level)?;
    } else {
        writeln!(w, "<div class=\"sect{}\">", section.level)?;
    }
    write!(w, "<h{level} id=\"{id}\">")?;

    // Special section styles (bibliography, glossary, etc.) should not be numbered
    let skip_numbering = section
        .metadata
        .style
        .as_ref()
        .is_some_and(|s| UNNUMBERED_SECTION_STYLES.contains(&s.as_str()));

    // Prepend section number if sectnums is enabled and this isn't a special section
    if !skip_numbering
        && let Some(number) = processor
            .section_number_tracker()
            .enter_section(section.level)
    {
        write!(w, "{number}")?;
    }

    let _ = w;
    visitor.visit_inline_nodes(&section.title)?;
    w = visitor.writer_mut();
    writeln!(w, "</h{level}>")?;

    // Only sect1 gets a sectionbody wrapper in standard mode
    if processor.variant() == HtmlVariant::Standard && section.level == 1 {
        writeln!(w, "<div class=\"sectionbody\">")?;
    }
    Ok(())
}

/// Render the section footer (closing tags)
///
/// Call this after walking the section's nested blocks.
fn render_section_footer<V: WritableVisitor<Error = Error>>(
    section: &Section,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), Error> {
    let w = visitor.writer_mut();

    if processor.variant() == HtmlVariant::Semantic {
        writeln!(w, "</section>")?;
    } else {
        // Only sect1 has a sectionbody wrapper to close
        if section.level == 1 {
            writeln!(w, "</div>")?; // Close sectionbody
        }
        writeln!(w, "</div>")?; // Close sectN
    }
    Ok(())
}

pub(crate) fn visit_discrete_header<V: WritableVisitor<Error = Error>>(
    header: &DiscreteHeader,
    visitor: &mut V,
) -> Result<(), Error> {
    let level = header.level + 1; // Level 1 = h2
    let id = Section::generate_id(&header.metadata, &header.title);

    let mut w = visitor.writer_mut();
    write!(w, "<h{level} id=\"{id}\" class=\"discrete\">")?;
    let _ = w;
    visitor.visit_inline_nodes(&header.title)?;
    w = visitor.writer_mut();
    writeln!(w, "</h{level}>")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::{AttributeValue, DocumentAttributes};

    fn attrs_with_sectnums() -> DocumentAttributes {
        let mut attrs = DocumentAttributes::default();
        attrs.insert("sectnums".to_string(), AttributeValue::Bool(true));
        attrs
    }

    fn attrs_with_numbered() -> DocumentAttributes {
        let mut attrs = DocumentAttributes::default();
        attrs.insert("numbered".to_string(), AttributeValue::Bool(true));
        attrs
    }

    fn attrs_with_sectnums_and_levels(levels: u8) -> DocumentAttributes {
        let mut attrs = attrs_with_sectnums();
        // Use set() instead of insert() to override the default value
        attrs.set(
            "sectnumlevels".to_string(),
            AttributeValue::String(levels.to_string()),
        );
        attrs
    }

    #[test]
    fn test_tracker_disabled_by_default() {
        let attrs = DocumentAttributes::default();
        let tracker = SectionNumberTracker::new(&attrs);

        // No numbering without :sectnums:
        assert!(tracker.enter_section(1).is_none());
        assert!(tracker.enter_section(2).is_none());
    }

    #[test]
    fn test_tracker_disabled_returns_none() {
        let mut attrs = DocumentAttributes::default();
        attrs.insert("sectnums".to_string(), AttributeValue::Bool(false));
        let tracker = SectionNumberTracker::new(&attrs);

        assert!(tracker.enter_section(1).is_none());
    }

    #[test]
    fn test_tracker_numbered_alias_enables_sectnums() {
        // :numbered: is a deprecated alias for :sectnums:
        let tracker = SectionNumberTracker::new(&attrs_with_numbered());

        assert_eq!(tracker.enter_section(1), Some("1. ".to_string()));
        assert_eq!(tracker.enter_section(1), Some("2. ".to_string()));
    }

    #[test]
    fn test_tracker_increments_correctly() {
        let tracker = SectionNumberTracker::new(&attrs_with_sectnums());

        assert_eq!(tracker.enter_section(1), Some("1. ".to_string()));
        assert_eq!(tracker.enter_section(1), Some("2. ".to_string()));
        assert_eq!(tracker.enter_section(1), Some("3. ".to_string()));
    }

    #[test]
    fn test_tracker_nested_numbering() {
        let tracker = SectionNumberTracker::new(&attrs_with_sectnums());

        assert_eq!(tracker.enter_section(1), Some("1. ".to_string()));
        assert_eq!(tracker.enter_section(2), Some("1.1. ".to_string()));
        assert_eq!(tracker.enter_section(2), Some("1.2. ".to_string()));
        assert_eq!(tracker.enter_section(3), Some("1.2.1. ".to_string()));
        assert_eq!(tracker.enter_section(1), Some("2. ".to_string()));
        assert_eq!(tracker.enter_section(2), Some("2.1. ".to_string()));
    }

    #[test]
    fn test_tracker_resets_deeper_levels() {
        let tracker = SectionNumberTracker::new(&attrs_with_sectnums());

        assert_eq!(tracker.enter_section(1), Some("1. ".to_string()));
        assert_eq!(tracker.enter_section(2), Some("1.1. ".to_string()));
        assert_eq!(tracker.enter_section(2), Some("1.2. ".to_string()));
        // Going back to level 1 should reset level 2
        assert_eq!(tracker.enter_section(1), Some("2. ".to_string()));
        // Level 2 should start from 1 again
        assert_eq!(tracker.enter_section(2), Some("2.1. ".to_string()));
    }

    #[test]
    fn test_tracker_respects_max_level() {
        // Default max level is 3
        let tracker = SectionNumberTracker::new(&attrs_with_sectnums());

        assert_eq!(tracker.enter_section(1), Some("1. ".to_string()));
        assert_eq!(tracker.enter_section(2), Some("1.1. ".to_string()));
        assert_eq!(tracker.enter_section(3), Some("1.1.1. ".to_string()));
        // Level 4 should not be numbered (beyond default sectnumlevels=3)
        assert!(tracker.enter_section(4).is_none());
    }

    #[test]
    fn test_tracker_custom_sectnumlevels() {
        let tracker = SectionNumberTracker::new(&attrs_with_sectnums_and_levels(2));

        assert_eq!(tracker.enter_section(1), Some("1. ".to_string()));
        assert_eq!(tracker.enter_section(2), Some("1.1. ".to_string()));
        // Level 3 should not be numbered when sectnumlevels=2
        assert!(tracker.enter_section(3).is_none());
    }

    #[test]
    fn test_tracker_sectnumlevels_zero_disables() {
        let tracker = SectionNumberTracker::new(&attrs_with_sectnums_and_levels(0));

        // sectnumlevels=0 means no sections get numbered
        assert!(tracker.enter_section(1).is_none());
    }

    #[test]
    fn test_tracker_level_zero_returns_none() {
        let tracker = SectionNumberTracker::new(&attrs_with_sectnums());

        // Level 0 is invalid
        assert!(tracker.enter_section(0).is_none());
    }

    #[test]
    fn test_tracker_high_levels() {
        // Test all 5 supported levels
        let tracker = SectionNumberTracker::new(&attrs_with_sectnums_and_levels(5));

        assert_eq!(tracker.enter_section(1), Some("1. ".to_string()));
        assert_eq!(tracker.enter_section(2), Some("1.1. ".to_string()));
        assert_eq!(tracker.enter_section(3), Some("1.1.1. ".to_string()));
        assert_eq!(tracker.enter_section(4), Some("1.1.1.1. ".to_string()));
        assert_eq!(tracker.enter_section(5), Some("1.1.1.1.1. ".to_string()));
    }

    #[test]
    fn test_tracker_clone_shares_state() {
        let tracker1 = SectionNumberTracker::new(&attrs_with_sectnums());
        let tracker2 = tracker1.clone();

        assert_eq!(tracker1.enter_section(1), Some("1. ".to_string()));
        // Clone should see the same counter state
        assert_eq!(tracker2.enter_section(1), Some("2. ".to_string()));
    }
}
