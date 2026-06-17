//! Section numbering, part numbering, and appendix tracking utilities.
//!
//! These trackers are shared across converters (HTML, terminal, etc.) to provide
//! consistent section numbering behavior. They use `Rc<Cell<>>` / `Rc<RefCell<>>`
//! for shared state across clones.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use acdc_parser::{AttributeValue, Block, DocumentAttributes, MAX_SECTION_LEVELS, SectionKind};

/// Whether the last section in `blocks` is tagged with the `[index]` style.
///
/// Used by converters to decide whether to defer index-term catalog rendering
/// until the document's explicit index section.
#[must_use]
pub fn last_section_has_style(blocks: &[Block<'_>], style: &str) -> bool {
    let last_section = blocks.iter().rev().find_map(|block| {
        if let Block::Section(section) = block {
            Some(section)
        } else {
            None
        }
    });
    last_section.is_some_and(|section| section.metadata.style.as_ref().is_some_and(|s| *s == style))
}

/// Default maximum section level for numbering.
pub const DEFAULT_SECTION_LEVEL: u8 = 3;

/// Decides which sections take part in `:sectnums:` numbering, accounting for
/// `AsciiDoc` special sections.
///
/// A special section (`[preface]`, `[glossary]`, …) is not numbered, and neither
/// are the subsections nested under it. `[appendix]` is special too, but it
/// begins its own (letter-based) numbered sequence, so it does *not* suppress
/// numbering for its descendants.
///
/// Feed every section to [`enter`](Self::enter) once, in document (pre-order)
/// order — the same order both the body walk and the flat TOC list visit
/// sections — so every converter applies the rule identically. State is shared
/// across clones (like the other trackers here) so a cloned `Processor` keeps a
/// single walk.
#[derive(Clone, Debug, Default)]
pub struct SpecialSectionTracker {
    /// Levels at which a still-open suppressing ancestor section started.
    suppress_levels: Rc<RefCell<Vec<u8>>>,
}

impl SpecialSectionTracker {
    /// Create a new tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record entering a section (in document order) and return whether it
    /// participates in `:sectnums:` numbering.
    ///
    /// Returns `false` for a special section and for any section nested under a
    /// non-appendix special section.
    #[must_use]
    pub fn enter(&self, level: u8, kind: SectionKind) -> bool {
        let mut stack = self.suppress_levels.borrow_mut();
        // Leaving any sibling-or-shallower subtree closes its suppression.
        while stack.last().is_some_and(|&started_at| level <= started_at) {
            stack.pop();
        }
        let inside_special = !stack.is_empty();
        // Special sections suppress their descendants — except appendix, whose
        // subsections continue its letter numbering.
        if kind.is_special() && kind != SectionKind::Appendix {
            stack.push(level);
        }
        !(inside_special || kind.is_special())
    }
}

/// Tracks section numbers for `:sectnums:` attribute support.
/// Maintains hierarchical counters (e.g., "1.", "1.1.", "1.1.1.").
#[derive(Clone, Debug)]
pub struct SectionNumberTracker {
    /// Counters for each level (index 0 = level 1, etc.)
    counters: Rc<RefCell<[usize; MAX_SECTION_LEVELS as usize + 1]>>,
    /// Whether section numbering is enabled
    enabled: Rc<Cell<bool>>,
    /// Maximum level to number (from `:sectnumlevels:`, default 3)
    max_level: u8,
    /// When inside an appendix subtree, the appendix's letter numeral (`A`, `B`,
    /// …). Subsection numbers use this letter as their top component (`A.1`,
    /// `A.1.1`) instead of a digit. Cleared when a normal top-level section
    /// resumes numeric numbering.
    appendix_letter: Rc<Cell<Option<char>>>,
}

impl SectionNumberTracker {
    /// Reset all section counters to zero.
    /// Used when entering a new part boundary to restart chapter numbering.
    pub fn reset(&self) {
        let mut counters = self.counters.borrow_mut();
        for c in counters.iter_mut() {
            *c = 0;
        }
        self.appendix_letter.set(None);
    }

    /// Begin an appendix subtree: record the appendix's letter numeral and zero
    /// the section counters so its subsections number as `A.1`, `A.1.1`, ….
    ///
    /// Called instead of [`reset`](Self::reset) when entering an `[appendix]` so
    /// the caption letter (`Appendix A:`) and the subsection numbers (`A.1`)
    /// agree on the same letter.
    pub fn enter_appendix_subtree(&self, letter: char) {
        let mut counters = self.counters.borrow_mut();
        for c in counters.iter_mut() {
            *c = 0;
        }
        self.appendix_letter.set(Some(letter));
    }

    /// Create a new section number tracker.
    #[must_use]
    pub fn new(document_attributes: &DocumentAttributes) -> Self {
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
            appendix_letter: Rc::new(Cell::new(None)),
        }
    }

    /// Enter a section and return its number if numbering is enabled.
    /// Returns None if numbering is disabled or section is beyond max level.
    ///
    /// Inside an appendix subtree (see [`enter_appendix_subtree`]) the top
    /// component is the appendix letter, so subsections number as `A.1`,
    /// `A.1.1`. A normal top-level section (`level == 1`) clears that state and
    /// resumes numeric numbering.
    ///
    /// [`enter_appendix_subtree`]: Self::enter_appendix_subtree
    #[must_use]
    pub fn enter_section(&self, level: u8) -> Option<String> {
        // A normal top-level section ends any appendix region.
        if level == 1 {
            self.appendix_letter.set(None);
        }

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

        // Build the number string. Inside an appendix subtree the level-1
        // component is the appendix letter (`A.1`), deeper levels stay numeric.
        let number: String = if let Some(letter) = self.appendix_letter.get() {
            std::iter::once(letter.to_string())
                .chain(counters.get(1..=level_idx)?.iter().map(ToString::to_string))
                .collect::<Vec<_>>()
                .join(".")
        } else {
            counters
                .get(..=level_idx)?
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(".")
        };

        Some(format!("{number}. "))
    }
}

/// Convert a number to uppercase Roman numerals using subtractive notation.
#[must_use]
pub fn to_upper_roman(mut n: usize) -> String {
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
pub struct PartNumberTracker {
    counter: Rc<Cell<usize>>,
    enabled: bool,
    signifier: Option<String>,
    section_tracker: SectionNumberTracker,
}

impl PartNumberTracker {
    /// Create a new part number tracker from document attributes.
    /// `section_tracker` should be a clone of the processor's `SectionNumberTracker`
    /// so they share state — entering a part resets section counters.
    #[must_use]
    pub fn new(
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
                AttributeValue::String(s) => Some(s.clone().into_owned()),
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
    #[must_use]
    pub fn enter_part(&self) -> Option<String> {
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
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get the part signifier text, if any.
    #[must_use]
    pub fn signifier(&self) -> Option<&str> {
        self.signifier.as_deref()
    }
}

/// Tracks appendix numbering for `[appendix]` style on level-0 sections in book doctype.
/// Appendix sections are demoted from level 0 to level 1 and prefixed with
/// "Appendix A: ", "Appendix B: ", etc.
#[derive(Clone, Debug)]
pub struct AppendixTracker {
    counter: Rc<Cell<usize>>,
    caption: Option<String>,
    section_tracker: SectionNumberTracker,
}

impl AppendixTracker {
    /// Create a new appendix tracker from document attributes.
    /// `section_tracker` should be a clone of the processor's `SectionNumberTracker`
    /// so they share state — entering an appendix resets section counters.
    #[must_use]
    pub fn new(
        document_attributes: &DocumentAttributes,
        section_tracker: SectionNumberTracker,
    ) -> Self {
        // :appendix-caption: defaults to "Appendix" unless explicitly set or negated
        let caption = match document_attributes.get("appendix-caption") {
            Some(AttributeValue::String(s)) => Some(s.clone().into_owned()),
            Some(AttributeValue::Bool(false)) => None,
            _ => Some("Appendix".to_string()),
        };

        Self {
            counter: Rc::new(Cell::new(0)),
            caption,
            section_tracker,
        }
    }

    /// Enter an appendix boundary. Returns the formatted heading prefix:
    /// `"Appendix A: "` with the caption, or the bare numeral `"A. "` when the
    /// caption is disabled (`:appendix-caption!:`). The letter prefix is shown
    /// regardless of `:sectnums:`.
    ///
    /// Also begins the appendix subtree on the shared section tracker so its
    /// subsections number as `A.1`, `A.1.1`, … using the same letter.
    #[must_use]
    pub fn enter_appendix(&self) -> String {
        let count = self.counter.get();
        self.counter.set(count + 1);

        let letter = char::from(b'A' + u8::try_from(count).unwrap_or(25).min(25));
        self.section_tracker.enter_appendix_subtree(letter);

        match self.caption.as_ref() {
            Some(caption) => format!("{caption} {letter}: "),
            None => format!("{letter}. "),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn special_tracker_numbers_normal_sections() {
        let tracker = SpecialSectionTracker::new();
        assert!(tracker.enter(1, SectionKind::Normal));
        assert!(tracker.enter(2, SectionKind::Normal));
        assert!(tracker.enter(1, SectionKind::Normal));
    }

    #[test]
    fn special_tracker_suppresses_preface_subtree() {
        let tracker = SpecialSectionTracker::new();
        // [preface] == Introduction
        assert!(!tracker.enter(1, SectionKind::Preface));
        // === Features (nested) — unnumbered by inheritance
        assert!(!tracker.enter(2, SectionKind::Normal));
        // ==== Deeper (still nested) — unnumbered
        assert!(!tracker.enter(3, SectionKind::Normal));
        // == Real Chapter (sibling of the preface) — numbering resumes
        assert!(tracker.enter(1, SectionKind::Normal));
        assert!(tracker.enter(2, SectionKind::Normal));
    }

    #[test]
    fn special_tracker_appendix_does_not_suppress_descendants() {
        let tracker = SpecialSectionTracker::new();
        // [appendix] == App — special, so not part of the normal numbered run
        assert!(!tracker.enter(1, SectionKind::Appendix));
        // === App Sub — appendix begins its own sequence, so the subsection is
        // not suppressed the way a preface subsection would be.
        assert!(tracker.enter(2, SectionKind::Normal));
    }

    #[test]
    fn special_tracker_handles_special_within_special() {
        let tracker = SpecialSectionTracker::new();
        assert!(!tracker.enter(1, SectionKind::Preface));
        // A nested special section is itself unnumbered, and so is its subtree.
        assert!(!tracker.enter(2, SectionKind::Abstract));
        assert!(!tracker.enter(3, SectionKind::Normal));
        // Back out to a normal top-level section.
        assert!(tracker.enter(1, SectionKind::Normal));
    }

    fn attrs_with_sectnums() -> DocumentAttributes<'static> {
        let mut attrs = DocumentAttributes::default();
        attrs.insert("sectnums".into(), AttributeValue::Bool(true));
        attrs
    }

    fn attrs_with_numbered() -> DocumentAttributes<'static> {
        let mut attrs = DocumentAttributes::default();
        attrs.insert("numbered".into(), AttributeValue::Bool(true));
        attrs
    }

    fn attrs_with_sectnums_and_levels(levels: u8) -> DocumentAttributes<'static> {
        let mut attrs = attrs_with_sectnums();
        attrs.set(
            "sectnumlevels".into(),
            AttributeValue::String(levels.to_string().into()),
        );
        attrs
    }

    #[test]
    fn test_tracker_disabled_by_default() {
        let attrs = DocumentAttributes::default();
        let tracker = SectionNumberTracker::new(&attrs);
        assert!(tracker.enter_section(1).is_none());
        assert!(tracker.enter_section(2).is_none());
    }

    #[test]
    fn test_tracker_disabled_returns_none() {
        let mut attrs = DocumentAttributes::default();
        attrs.insert("sectnums".into(), AttributeValue::Bool(false));
        let tracker = SectionNumberTracker::new(&attrs);
        assert!(tracker.enter_section(1).is_none());
    }

    #[test]
    fn test_tracker_numbered_alias_enables_sectnums() {
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
        assert_eq!(tracker.enter_section(1), Some("2. ".to_string()));
        assert_eq!(tracker.enter_section(2), Some("2.1. ".to_string()));
    }

    #[test]
    fn test_tracker_respects_max_level() {
        let tracker = SectionNumberTracker::new(&attrs_with_sectnums());
        assert_eq!(tracker.enter_section(1), Some("1. ".to_string()));
        assert_eq!(tracker.enter_section(2), Some("1.1. ".to_string()));
        assert_eq!(tracker.enter_section(3), Some("1.1.1. ".to_string()));
        assert!(tracker.enter_section(4).is_none());
    }

    #[test]
    fn test_tracker_custom_sectnumlevels() {
        let tracker = SectionNumberTracker::new(&attrs_with_sectnums_and_levels(2));
        assert_eq!(tracker.enter_section(1), Some("1. ".to_string()));
        assert_eq!(tracker.enter_section(2), Some("1.1. ".to_string()));
        assert!(tracker.enter_section(3).is_none());
    }

    #[test]
    fn test_tracker_sectnumlevels_zero_disables() {
        let tracker = SectionNumberTracker::new(&attrs_with_sectnums_and_levels(0));
        assert!(tracker.enter_section(1).is_none());
    }

    #[test]
    fn test_tracker_level_zero_returns_none() {
        let tracker = SectionNumberTracker::new(&attrs_with_sectnums());
        assert!(tracker.enter_section(0).is_none());
    }

    #[test]
    fn test_tracker_high_levels() {
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

    fn attrs_with_partnums() -> DocumentAttributes<'static> {
        let mut attrs = attrs_with_sectnums();
        attrs.insert("doctype".into(), AttributeValue::String("book".into()));
        attrs.insert("partnums".into(), AttributeValue::Bool(true));
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
            "part-signifier".into(),
            AttributeValue::String("Part".into()),
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
        let _ = part_tracker.enter_part();
        assert_eq!(section_tracker.enter_section(1), Some("1. ".to_string()));
        assert_eq!(section_tracker.enter_section(1), Some("2. ".to_string()));
        let _ = part_tracker.enter_part();
        assert_eq!(section_tracker.enter_section(1), Some("1. ".to_string()));
    }

    #[test]
    fn test_appendix_tracker_default_caption() {
        let attrs = DocumentAttributes::default();
        let section_tracker = SectionNumberTracker::new(&attrs);
        let tracker = AppendixTracker::new(&attrs, section_tracker);
        assert_eq!(tracker.enter_appendix(), "Appendix A: ");
        assert_eq!(tracker.enter_appendix(), "Appendix B: ");
        assert_eq!(tracker.enter_appendix(), "Appendix C: ");
    }

    #[test]
    fn test_appendix_tracker_custom_caption() {
        let mut attrs = DocumentAttributes::default();
        attrs.set(
            "appendix-caption".into(),
            AttributeValue::String("Annexe".into()),
        );
        let section_tracker = SectionNumberTracker::new(&attrs);
        let tracker = AppendixTracker::new(&attrs, section_tracker);
        assert_eq!(tracker.enter_appendix(), "Annexe A: ");
        assert_eq!(tracker.enter_appendix(), "Annexe B: ");
    }

    #[test]
    fn test_appendix_tracker_disabled_caption() {
        // With the caption disabled (`:appendix-caption!:`) the appendix still
        // gets its bare letter numeral (`A.`, `B.`), matching asciidoctor.
        let mut attrs = DocumentAttributes::default();
        attrs.set("appendix-caption".into(), AttributeValue::Bool(false));
        let section_tracker = SectionNumberTracker::new(&attrs);
        let tracker = AppendixTracker::new(&attrs, section_tracker);
        assert_eq!(tracker.enter_appendix(), "A. ");
        assert_eq!(tracker.enter_appendix(), "B. ");
    }

    #[test]
    fn test_appendix_subtree_numbers_subsections_with_letter() {
        // Appendix subsections number as `A.1`, `A.1.1`, `A.2`, then `B.1` for
        // the next appendix, matching asciidoctor.
        let attrs = attrs_with_sectnums();
        let section_tracker = SectionNumberTracker::new(&attrs);
        let appendix_tracker = AppendixTracker::new(&attrs, section_tracker.clone());

        assert_eq!(appendix_tracker.enter_appendix(), "Appendix A: ");
        assert_eq!(section_tracker.enter_section(2), Some("A.1. ".to_string()));
        assert_eq!(
            section_tracker.enter_section(3),
            Some("A.1.1. ".to_string())
        );
        assert_eq!(section_tracker.enter_section(2), Some("A.2. ".to_string()));

        assert_eq!(appendix_tracker.enter_appendix(), "Appendix B: ");
        assert_eq!(section_tracker.enter_section(2), Some("B.1. ".to_string()));
    }

    #[test]
    fn test_normal_section_clears_appendix_letter() {
        // A normal top-level section after an appendix resumes numeric numbering.
        let attrs = attrs_with_sectnums();
        let section_tracker = SectionNumberTracker::new(&attrs);
        let appendix_tracker = AppendixTracker::new(&attrs, section_tracker.clone());

        let _ = appendix_tracker.enter_appendix();
        assert_eq!(section_tracker.enter_section(2), Some("A.1. ".to_string()));
        assert_eq!(section_tracker.enter_section(1), Some("1. ".to_string()));
        assert_eq!(section_tracker.enter_section(2), Some("1.1. ".to_string()));
    }

    #[test]
    fn test_appendix_subtree_respects_sectnumlevels() {
        // :sectnumlevels: caps appendix subsections just like normal sections.
        let attrs = attrs_with_sectnums_and_levels(2);
        let section_tracker = SectionNumberTracker::new(&attrs);
        let appendix_tracker = AppendixTracker::new(&attrs, section_tracker.clone());

        let _ = appendix_tracker.enter_appendix();
        assert_eq!(section_tracker.enter_section(2), Some("A.1. ".to_string()));
        // Level 3 is beyond sectnumlevels=2, so unnumbered.
        assert!(section_tracker.enter_section(3).is_none());
    }

    #[test]
    fn test_appendix_tracker_resets_section_counters() {
        let attrs = attrs_with_sectnums();
        let section_tracker = SectionNumberTracker::new(&attrs);
        let appendix_tracker = AppendixTracker::new(&attrs, section_tracker.clone());
        assert_eq!(section_tracker.enter_section(1), Some("1. ".to_string()));
        assert_eq!(section_tracker.enter_section(1), Some("2. ".to_string()));
        let _ = appendix_tracker.enter_appendix();
        assert_eq!(section_tracker.enter_section(1), Some("1. ".to_string()));
    }
}
