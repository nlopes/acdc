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
    /// Reset all section counters to zero.
    /// Used when entering a new part boundary to restart chapter numbering.
    pub(crate) fn reset(&self) {
        let mut counters = self.counters.borrow_mut();
        for c in counters.iter_mut() {
            *c = 0;
        }
    }

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

/// Convert a number to uppercase Roman numerals using subtractive notation.
pub(crate) fn to_upper_roman(mut n: usize) -> String {
    const TABLE: &[(usize, &str)] = &[
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut result = String::new();
    for &(value, numeral) in TABLE {
        while n >= value {
            result.push_str(numeral);
            n -= value;
        }
    }
    result
}

/// Tracks part numbers for `:partnums:` attribute support in book doctype.
/// Formats part headings as "Part I. ", "Part II. ", etc.
#[derive(Clone, Debug)]
pub(crate) struct PartNumberTracker {
    counter: Rc<Cell<usize>>,
    enabled: bool,
    signifier: Option<String>,
    section_tracker: SectionNumberTracker,
}

impl PartNumberTracker {
    /// Create a new part number tracker from document attributes.
    /// `section_tracker` should be a clone of the processor's `SectionNumberTracker`
    /// so they share state — entering a part resets section counters.
    pub(crate) fn new(
        document_attributes: &DocumentAttributes,
        section_tracker: SectionNumberTracker,
    ) -> Self {
        let is_book = document_attributes
            .get("doctype")
            .is_some_and(|v| v.to_string() == "book");
        let enabled = is_book && document_attributes.contains_key("partnums");

        // :part-signifier: defaults to None (no prefix text before the Roman numeral)
        // If set, e.g. :part-signifier: Part, produces "Part I. "
        // If negated (:!part-signifier:), also None
        let signifier = document_attributes
            .get("part-signifier")
            .and_then(|v| match v {
                AttributeValue::String(s) => Some(s.clone()),
                AttributeValue::Bool(_) | AttributeValue::None | _ => None,
            });

        Self {
            counter: Rc::new(Cell::new(0)),
            enabled,
            signifier,
            section_tracker,
        }
    }

    /// Enter a part boundary. Returns the formatted part label (e.g. "Part I. ")
    /// if part numbering is enabled, or `None` otherwise.
    /// Also resets section counters for the new part.
    pub(crate) fn enter_part(&self) -> Option<String> {
        if !self.enabled {
            return None;
        }
        let count = self.counter.get() + 1;
        self.counter.set(count);
        self.section_tracker.reset();

        let roman = to_upper_roman(count);
        if let Some(ref sig) = self.signifier {
            Some(format!("{sig} {roman}: "))
        } else {
            Some(format!("{roman}: "))
        }
    }

    /// Whether part numbering is enabled (`:partnums:` is set).
    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get the part signifier text, if any.
    pub(crate) fn signifier(&self) -> Option<&str> {
        self.signifier.as_deref()
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

    // Special section styles (bibliography, glossary, etc.) should not be numbered
    let skip_numbering = section
        .metadata
        .style
        .as_ref()
        .is_some_and(|s| UNNUMBERED_SECTION_STYLES.contains(&s.as_str()));

    let mut w = visitor.writer_mut();

    if section.level == 0 {
        // Parts (level 0) in book doctype: standalone h1 with class="sect0", no wrapper div
        write!(w, "<h{level} id=\"{id}\" class=\"sect0\">")?;

        // Prepend part number if :partnums: is enabled
        if !skip_numbering && let Some(part_label) = processor.part_number_tracker().enter_part() {
            write!(w, "{part_label}")?;
        }
    } else {
        if processor.variant() == HtmlVariant::Semantic {
            writeln!(w, "<section class=\"doc-section level-{}\">", section.level)?;
        } else {
            writeln!(w, "<div class=\"sect{}\">", section.level)?;
        }
        write!(w, "<h{level} id=\"{id}\">")?;

        // Prepend section number if sectnums is enabled and this isn't a special section
        if !skip_numbering
            && let Some(number) = processor
                .section_number_tracker()
                .enter_section(section.level)
        {
            write!(w, "{number}")?;
        }
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
    // Parts (level 0) have no wrapper div to close
    if section.level == 0 {
        return Ok(());
    }

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

    #[test]
    fn test_tracker_reset() {
        let tracker = SectionNumberTracker::new(&attrs_with_sectnums());

        assert_eq!(tracker.enter_section(1), Some("1. ".to_string()));
        assert_eq!(tracker.enter_section(2), Some("1.1. ".to_string()));
        tracker.reset();
        assert_eq!(tracker.enter_section(1), Some("1. ".to_string()));
        assert_eq!(tracker.enter_section(2), Some("1.1. ".to_string()));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Roman numeral tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_to_upper_roman() {
        assert_eq!(to_upper_roman(1), "I");
        assert_eq!(to_upper_roman(2), "II");
        assert_eq!(to_upper_roman(3), "III");
        assert_eq!(to_upper_roman(4), "IV");
        assert_eq!(to_upper_roman(5), "V");
        assert_eq!(to_upper_roman(9), "IX");
        assert_eq!(to_upper_roman(10), "X");
        assert_eq!(to_upper_roman(14), "XIV");
        assert_eq!(to_upper_roman(40), "XL");
        assert_eq!(to_upper_roman(49), "XLIX");
        assert_eq!(to_upper_roman(99), "XCIX");
        assert_eq!(to_upper_roman(399), "CCCXCIX");
        assert_eq!(to_upper_roman(1994), "MCMXCIV");
        assert_eq!(to_upper_roman(3999), "MMMCMXCIX");
    }

    #[test]
    fn test_to_upper_roman_zero() {
        assert_eq!(to_upper_roman(0), "");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // PartNumberTracker tests
    // ─────────────────────────────────────────────────────────────────────────

    fn attrs_with_partnums() -> DocumentAttributes {
        let mut attrs = attrs_with_sectnums();
        attrs.insert(
            "doctype".to_string(),
            AttributeValue::String("book".to_string()),
        );
        attrs.insert("partnums".to_string(), AttributeValue::Bool(true));
        attrs
    }

    #[test]
    fn test_part_tracker_disabled_without_partnums() {
        let attrs = DocumentAttributes::default();
        let section_tracker = SectionNumberTracker::new(&attrs);
        let tracker = PartNumberTracker::new(&attrs, section_tracker);

        assert!(!tracker.is_enabled());
        assert!(tracker.enter_part().is_none());
    }

    #[test]
    fn test_part_tracker_enabled_no_signifier() {
        let attrs = attrs_with_partnums();
        let section_tracker = SectionNumberTracker::new(&attrs);
        let tracker = PartNumberTracker::new(&attrs, section_tracker);

        assert!(tracker.is_enabled());
        assert!(tracker.signifier().is_none());
        assert_eq!(tracker.enter_part(), Some("I: ".to_string()));
        assert_eq!(tracker.enter_part(), Some("II: ".to_string()));
        assert_eq!(tracker.enter_part(), Some("III: ".to_string()));
    }

    #[test]
    fn test_part_tracker_with_signifier() {
        let mut attrs = attrs_with_partnums();
        attrs.insert(
            "part-signifier".to_string(),
            AttributeValue::String("Part".to_string()),
        );
        let section_tracker = SectionNumberTracker::new(&attrs);
        let tracker = PartNumberTracker::new(&attrs, section_tracker);

        assert_eq!(tracker.signifier(), Some("Part"));
        assert_eq!(tracker.enter_part(), Some("Part I: ".to_string()));
        assert_eq!(tracker.enter_part(), Some("Part II: ".to_string()));
    }

    #[test]
    fn test_part_tracker_resets_section_counters() {
        let attrs = attrs_with_partnums();
        let section_tracker = SectionNumberTracker::new(&attrs);
        let part_tracker = PartNumberTracker::new(&attrs, section_tracker.clone());

        // Enter first part, then some sections
        part_tracker.enter_part();
        assert_eq!(section_tracker.enter_section(1), Some("1. ".to_string()));
        assert_eq!(section_tracker.enter_section(1), Some("2. ".to_string()));

        // Enter second part — should reset section counters
        part_tracker.enter_part();
        assert_eq!(section_tracker.enter_section(1), Some("1. ".to_string()));
    }
}
