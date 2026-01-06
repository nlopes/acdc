//! Substitution types and application for `AsciiDoc` content.
//!
//! # Architecture: Parser vs Converter Responsibilities
//!
//! Substitutions are split between the parser and converters by design:
//!
//! ## Parser handles (format-agnostic)
//!
//! - **Attributes** - Expands `{name}` references using document attributes.
//!   This is document-wide and doesn't depend on output format.
//!
//! - **Group expansion** - `Normal` and `Verbatim` expand to their constituent
//!   substitution lists recursively.
//!
//! ## Converters handle (format-specific)
//!
//! - **`SpecialChars`** - HTML converter escapes `<`, `>`, `&` to entities.
//!   Other converters may handle differently (e.g., terminal needs no escaping).
//!
//! - **Quotes** - Parses inline formatting (`*bold*`, `_italic_`, etc.) via
//!   [`crate::parse_text_for_quotes`]. The converter then renders the parsed
//!   nodes appropriately for the output format.
//!
//! - **Replacements** - Typography transformations (em-dashes, arrows, ellipsis).
//!   Output varies by format (HTML entities vs Unicode characters).
//!
//! - **Callouts** - Already parsed into [`crate::CalloutRef`] nodes by the grammar.
//!   Converters render the callout markers.
//!
//! - **Macros** / **`PostReplacements`** - Not yet implemented.
//!
//! ## Why this split?
//!
//! The parser stays format-agnostic. It extracts the substitution list from
//! `[subs=...]` attributes and stores it in the AST. Each converter then
//! applies the relevant substitutions for its output format. This allows
//! adding new converters (terminal, manpage, PDF) without modifying the parser.
//!
//! ## Usage flow
//!
//! 1. Parser extracts `subs=` attribute → stored in [`crate::BlockMetadata`]
//! 2. Parser applies `Attributes` substitution during parsing
//! 3. Converter reads the substitution list from AST
//! 4. Converter applies remaining substitutions during rendering

use serde::{Deserialize, Serialize};

use crate::{AttributeValue, DocumentAttributes};

/// A `Substitution` represents a substitution in a passthrough macro.
#[derive(Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Substitution {
    SpecialChars,
    Attributes,
    Replacements,
    Macros,
    PostReplacements,
    Normal,
    Verbatim,
    Quotes,
    Callouts,
}

/// Parse a substitution name into a `Substitution` enum variant.
///
/// Returns `None` for unknown substitution types, which are logged and skipped.
pub(crate) fn parse_substitution(value: &str) -> Option<Substitution> {
    match value {
        "attributes" | "a" => Some(Substitution::Attributes),
        "replacements" | "r" => Some(Substitution::Replacements),
        "macros" | "m" => Some(Substitution::Macros),
        "post_replacements" | "p" => Some(Substitution::PostReplacements),
        "normal" | "n" => Some(Substitution::Normal),
        "verbatim" | "v" => Some(Substitution::Verbatim),
        "quotes" | "q" => Some(Substitution::Quotes),
        "callouts" => Some(Substitution::Callouts),
        "specialchars" | "c" => Some(Substitution::SpecialChars),
        unknown => {
            tracing::error!(
                substitution = %unknown,
                "unknown substitution type, ignoring - check for typos"
            );
            None
        }
    }
}

/// Default substitutions for header content.
pub const HEADER: &[Substitution] = &[Substitution::SpecialChars, Substitution::Attributes];

/// Default substitutions for normal content (paragraphs, etc).
pub const NORMAL: &[Substitution] = &[
    Substitution::SpecialChars,
    Substitution::Attributes,
    Substitution::Quotes,
    Substitution::Replacements,
    Substitution::Macros,
    Substitution::PostReplacements,
];

/// Default substitutions for verbatim blocks (listing, literal).
pub const VERBATIM: &[Substitution] = &[Substitution::SpecialChars, Substitution::Callouts];

/// Modifier for a substitution in the `subs` attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SubsModifier {
    /// `+name` - append to end of default list
    Append,
    /// `name+` - prepend to beginning of default list
    Prepend,
    /// `-name` - remove from default list
    Remove,
}

/// Parse a single subs part into name and optional modifier.
fn parse_subs_part(part: &str) -> (&str, Option<SubsModifier>) {
    if let Some(name) = part.strip_prefix('+') {
        (name, Some(SubsModifier::Append))
    } else if let Some(name) = part.strip_suffix('+') {
        (name, Some(SubsModifier::Prepend))
    } else if let Some(name) = part.strip_prefix('-') {
        (name, Some(SubsModifier::Remove))
    } else {
        (part, None)
    }
}

/// Parse a `subs` attribute value into an ordered list of substitutions.
///
/// Supports:
/// - `none` → empty list (no substitutions)
/// - `normal` → NORMAL list
/// - `verbatim` → VERBATIM list
/// - `a,q,c` → specific substitutions (comma-separated)
/// - `+quotes` → append to end of default list
/// - `quotes+` → prepend to beginning of default list
/// - `-specialchars` → remove from default list
/// - `specialchars,+quotes` → mixed: modifier mode with plain names
///
/// Order matters: substitutions are applied in sequence.
/// For modifier syntax (`+`/`-`), a default list must be provided.
#[must_use]
pub(crate) fn parse_subs_attribute(value: &str, default: &[Substitution]) -> Vec<Substitution> {
    let value = value.trim();

    // Handle special cases
    if value.is_empty() || value == "none" {
        return Vec::new();
    }

    // Parse all parts in one pass: O(n)
    let parts: Vec<_> = value
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(parse_subs_part)
        .collect();

    // Determine mode: if ANY part has a modifier, use modifier mode
    let has_modifiers = parts.iter().any(|(_, m)| m.is_some());

    if has_modifiers {
        // Modifier mode: start with defaults, apply modifiers
        let mut result: Vec<Substitution> = default.to_vec();

        for (name, modifier) in parts {
            // Parse the substitution name; skip if invalid
            let Some(sub) = parse_substitution(name) else {
                continue;
            };

            match modifier {
                Some(SubsModifier::Append) => {
                    append_substitution(&mut result, sub);
                }
                Some(SubsModifier::Prepend) => {
                    prepend_substitution(&mut result, sub);
                }
                Some(SubsModifier::Remove) => {
                    remove_substitution(&mut result, &sub);
                }
                None => {
                    // Plain substitution name in modifier context - warn and append
                    tracing::warn!(
                        substitution = %name,
                        "plain substitution in modifier context; consider +{name} for clarity"
                    );
                    append_substitution(&mut result, sub);
                }
            }
        }
        result
    } else {
        // No modifiers - parse as a list of substitution names (in order)
        let mut result = Vec::new();
        for (name, _) in parts {
            if let Some(sub) = parse_substitution(name) {
                append_substitution(&mut result, sub);
            }
        }
        result
    }
}

/// Append a substitution (or group) to the end of the list.
fn append_substitution(result: &mut Vec<Substitution>, sub: Substitution) {
    match sub {
        Substitution::Normal => {
            for s in NORMAL {
                if !result.contains(s) {
                    result.push(s.clone());
                }
            }
        }
        Substitution::Verbatim => {
            for s in VERBATIM {
                if !result.contains(s) {
                    result.push(s.clone());
                }
            }
        }
        Substitution::SpecialChars
        | Substitution::Attributes
        | Substitution::Replacements
        | Substitution::Macros
        | Substitution::PostReplacements
        | Substitution::Quotes
        | Substitution::Callouts => {
            if !result.contains(&sub) {
                result.push(sub);
            }
        }
    }
}

/// Prepend a substitution (or group) to the beginning of the list.
fn prepend_substitution(result: &mut Vec<Substitution>, sub: Substitution) {
    match sub {
        Substitution::Normal => {
            // Insert in reverse order at position 0 to maintain NORMAL order
            for s in NORMAL.iter().rev() {
                if !result.contains(s) {
                    result.insert(0, s.clone());
                }
            }
        }
        Substitution::Verbatim => {
            for s in VERBATIM.iter().rev() {
                if !result.contains(s) {
                    result.insert(0, s.clone());
                }
            }
        }
        Substitution::SpecialChars
        | Substitution::Attributes
        | Substitution::Replacements
        | Substitution::Macros
        | Substitution::PostReplacements
        | Substitution::Quotes
        | Substitution::Callouts => {
            if !result.contains(&sub) {
                result.insert(0, sub);
            }
        }
    }
}

/// Remove a substitution (or group) from the list.
fn remove_substitution(result: &mut Vec<Substitution>, sub: &Substitution) {
    match sub {
        Substitution::Normal => {
            for s in NORMAL {
                result.retain(|x| x != s);
            }
        }
        Substitution::Verbatim => {
            for s in VERBATIM {
                result.retain(|x| x != s);
            }
        }
        Substitution::SpecialChars
        | Substitution::Attributes
        | Substitution::Replacements
        | Substitution::Macros
        | Substitution::PostReplacements
        | Substitution::Quotes
        | Substitution::Callouts => {
            result.retain(|x| x != sub);
        }
    }
}

/// Apply a sequence of substitutions to text.
///
/// Iterates through the substitution list and applies each in order:
///
/// - `Attributes` - Expands `{name}` references using document attributes
/// - `Normal` / `Verbatim` - Recursively applies the corresponding substitution group
/// - All others (`SpecialChars`, `Quotes`, `Replacements`, `Macros`,
///   `PostReplacements`, `Callouts`) - No-op; handled by converters
///
/// # Example
///
/// ```
/// use acdc_parser::{DocumentAttributes, AttributeValue, Substitution, substitute};
///
/// let mut attrs = DocumentAttributes::default();
/// attrs.set("version".to_string(), AttributeValue::String("1.0".to_string()));
///
/// let result = substitute("Version {version}", &[Substitution::Attributes], &attrs);
/// assert_eq!(result, "Version 1.0");
/// ```
#[must_use]
pub fn substitute(
    text: &str,
    substitutions: &[Substitution],
    attributes: &DocumentAttributes,
) -> String {
    let mut result = text.to_string();
    for substitution in substitutions {
        match substitution {
            Substitution::Attributes => {
                // Expand {name} patterns with values from document attributes
                let mut expanded = String::with_capacity(result.len());
                let mut chars = result.chars().peekable();

                while let Some(ch) = chars.next() {
                    if ch == '{' {
                        let mut attr_name = String::new();
                        let mut found_closing_brace = false;

                        while let Some(&next_ch) = chars.peek() {
                            if next_ch == '}' {
                                chars.next();
                                found_closing_brace = true;
                                break;
                            }
                            attr_name.push(next_ch);
                            chars.next();
                        }

                        if found_closing_brace {
                            match attributes.get(&attr_name) {
                                Some(AttributeValue::Bool(true)) => {
                                    // Boolean true attributes expand to empty string
                                }
                                Some(AttributeValue::String(attr_value)) => {
                                    expanded.push_str(attr_value);
                                }
                                _ => {
                                    // Unknown attribute - keep reference as-is
                                    expanded.push('{');
                                    expanded.push_str(&attr_name);
                                    expanded.push('}');
                                }
                            }
                        } else {
                            // No closing brace - keep opening brace and collected chars
                            expanded.push('{');
                            expanded.push_str(&attr_name);
                        }
                    } else {
                        expanded.push(ch);
                    }
                }
                result = expanded;
            }
            // These substitutions are handled elsewhere (converter) or not yet implemented
            Substitution::SpecialChars
            | Substitution::Quotes
            | Substitution::Replacements
            | Substitution::Macros
            | Substitution::PostReplacements
            | Substitution::Callouts => {}
            // Group substitutions expand recursively
            Substitution::Normal => {
                result = substitute(&result, NORMAL, attributes);
            }
            Substitution::Verbatim => {
                result = substitute(&result, VERBATIM, attributes);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== Tests for parse_subs_attribute =====

    #[test]
    fn test_parse_subs_none() {
        let result = parse_subs_attribute("none", VERBATIM);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_subs_empty_string() {
        let result = parse_subs_attribute("", VERBATIM);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_subs_none_with_whitespace() {
        let result = parse_subs_attribute("  none  ", VERBATIM);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_subs_specialchars() {
        let result = parse_subs_attribute("specialchars", VERBATIM);
        assert_eq!(result, vec![Substitution::SpecialChars]);
    }

    #[test]
    fn test_parse_subs_specialchars_shorthand() {
        let result = parse_subs_attribute("c", VERBATIM);
        assert_eq!(result, vec![Substitution::SpecialChars]);
    }

    #[test]
    fn test_parse_subs_normal_expands() {
        let result = parse_subs_attribute("normal", &[]);
        assert_eq!(result, NORMAL.to_vec());
    }

    #[test]
    fn test_parse_subs_verbatim_expands() {
        let result = parse_subs_attribute("verbatim", &[]);
        assert_eq!(result, VERBATIM.to_vec());
    }

    #[test]
    fn test_parse_subs_append_modifier() {
        let result = parse_subs_attribute("+quotes", VERBATIM);
        // Should have VERBATIM (SpecialChars, Callouts) + Quotes at end
        assert!(result.contains(&Substitution::SpecialChars));
        assert!(result.contains(&Substitution::Callouts));
        assert!(result.contains(&Substitution::Quotes));
        assert_eq!(result.last(), Some(&Substitution::Quotes));
    }

    #[test]
    fn test_parse_subs_prepend_modifier() {
        let result = parse_subs_attribute("quotes+", VERBATIM);
        // Quotes should be at beginning
        assert_eq!(result.first(), Some(&Substitution::Quotes));
        assert!(result.contains(&Substitution::SpecialChars));
        assert!(result.contains(&Substitution::Callouts));
    }

    #[test]
    fn test_parse_subs_remove_modifier() {
        let result = parse_subs_attribute("-specialchars", VERBATIM);
        assert!(!result.contains(&Substitution::SpecialChars));
        assert!(result.contains(&Substitution::Callouts));
    }

    #[test]
    fn test_parse_subs_remove_all_verbatim() {
        let result = parse_subs_attribute("-specialchars,-callouts", VERBATIM);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_subs_combined_modifiers() {
        let result = parse_subs_attribute("+quotes,-callouts", VERBATIM);
        assert!(result.contains(&Substitution::SpecialChars)); // from default
        assert!(result.contains(&Substitution::Quotes)); // added
        assert!(!result.contains(&Substitution::Callouts)); // removed
    }

    #[test]
    fn test_parse_subs_ordering_preserved() {
        let result = parse_subs_attribute("quotes,attributes,specialchars", &[]);
        assert_eq!(
            result,
            vec![
                Substitution::Quotes,
                Substitution::Attributes,
                Substitution::SpecialChars
            ]
        );
    }

    #[test]
    fn test_parse_subs_shorthand_list() {
        let result = parse_subs_attribute("q,a,c", &[]);
        assert_eq!(
            result,
            vec![
                Substitution::Quotes,
                Substitution::Attributes,
                Substitution::SpecialChars
            ]
        );
    }

    #[test]
    fn test_parse_subs_with_spaces() {
        let result = parse_subs_attribute(" quotes , attributes ", &[]);
        assert_eq!(result, vec![Substitution::Quotes, Substitution::Attributes]);
    }

    #[test]
    fn test_parse_subs_duplicates_ignored() {
        let result = parse_subs_attribute("quotes,quotes,quotes", &[]);
        assert_eq!(result, vec![Substitution::Quotes]);
    }

    #[test]
    fn test_parse_subs_normal_in_list_expands() {
        let result = parse_subs_attribute("normal", &[]);
        // Should expand to all NORMAL substitutions
        assert_eq!(result.len(), NORMAL.len());
        for sub in NORMAL {
            assert!(result.contains(sub));
        }
    }

    #[test]
    fn test_parse_subs_append_normal_group() {
        let result = parse_subs_attribute("+normal", &[Substitution::Callouts]);
        // Should have Callouts + all of NORMAL
        assert!(result.contains(&Substitution::Callouts));
        for sub in NORMAL {
            assert!(result.contains(sub));
        }
    }

    #[test]
    fn test_parse_subs_remove_normal_group() {
        let result = parse_subs_attribute("-normal", NORMAL);
        // Removing normal group should leave empty
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_subs_unknown_is_skipped() {
        // Unknown substitution types are logged and skipped
        let result = parse_subs_attribute("unknown", &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_subs_unknown_mixed_with_valid() {
        // Unknown substitution types are skipped, valid ones are kept
        let result = parse_subs_attribute("quotes,typo,attributes", &[]);
        assert_eq!(result, vec![Substitution::Quotes, Substitution::Attributes]);
    }

    #[test]
    fn test_parse_subs_all_individual_types() {
        // Test each substitution type can be parsed
        assert_eq!(
            parse_subs_attribute("attributes", &[]),
            vec![Substitution::Attributes]
        );
        assert_eq!(
            parse_subs_attribute("replacements", &[]),
            vec![Substitution::Replacements]
        );
        assert_eq!(
            parse_subs_attribute("macros", &[]),
            vec![Substitution::Macros]
        );
        assert_eq!(
            parse_subs_attribute("post_replacements", &[]),
            vec![Substitution::PostReplacements]
        );
        assert_eq!(
            parse_subs_attribute("quotes", &[]),
            vec![Substitution::Quotes]
        );
        assert_eq!(
            parse_subs_attribute("callouts", &[]),
            vec![Substitution::Callouts]
        );
    }

    #[test]
    fn test_parse_subs_shorthand_types() {
        assert_eq!(
            parse_subs_attribute("a", &[]),
            vec![Substitution::Attributes]
        );
        assert_eq!(
            parse_subs_attribute("r", &[]),
            vec![Substitution::Replacements]
        );
        assert_eq!(parse_subs_attribute("m", &[]), vec![Substitution::Macros]);
        assert_eq!(
            parse_subs_attribute("p", &[]),
            vec![Substitution::PostReplacements]
        );
        assert_eq!(parse_subs_attribute("q", &[]), vec![Substitution::Quotes]);
        assert_eq!(
            parse_subs_attribute("c", &[]),
            vec![Substitution::SpecialChars]
        );
    }

    #[test]
    fn test_parse_subs_mixed_modifier_list() {
        // Bug case: subs=specialchars,+quotes - modifier not at start of string
        let result = parse_subs_attribute("specialchars,+quotes", VERBATIM);
        // Should be in modifier mode: VERBATIM defaults + quotes appended
        assert!(result.contains(&Substitution::SpecialChars));
        assert!(result.contains(&Substitution::Callouts)); // from VERBATIM default
        assert!(result.contains(&Substitution::Quotes)); // appended
    }

    #[test]
    fn test_parse_subs_modifier_in_middle() {
        // subs=attributes,+quotes,-callouts
        let result = parse_subs_attribute("attributes,+quotes,-callouts", VERBATIM);
        assert!(result.contains(&Substitution::Attributes)); // plain name in modifier context
        assert!(result.contains(&Substitution::Quotes)); // appended
        assert!(!result.contains(&Substitution::Callouts)); // removed
    }

    #[test]
    fn test_parse_subs_asciidoctor_example() {
        // From asciidoctor docs: subs="attributes+,+replacements,-callouts"
        let result = parse_subs_attribute("attributes+,+replacements,-callouts", VERBATIM);
        assert_eq!(result.first(), Some(&Substitution::Attributes)); // prepended
        assert!(result.contains(&Substitution::Replacements)); // appended
        assert!(!result.contains(&Substitution::Callouts)); // removed
    }

    #[test]
    fn test_parse_subs_modifier_only_at_end() {
        // Modifier at end of comma-separated list
        let result = parse_subs_attribute("quotes,-specialchars", VERBATIM);
        // Should detect modifier mode from -specialchars
        assert!(result.contains(&Substitution::Quotes)); // plain name appended
        assert!(!result.contains(&Substitution::SpecialChars)); // removed
        assert!(result.contains(&Substitution::Callouts)); // from default
    }

    // ===== Tests for substitute =====

    #[test]
    fn test_resolve_attribute_references() {
        // These two are attributes we add to the attributes map.
        let attribute_weight = AttributeValue::String(String::from("weight"));
        let attribute_mass = AttributeValue::String(String::from("mass"));

        // This one is an attribute we do NOT add to the attributes map so it can never be
        // resolved.
        let attribute_volume_repeat = String::from("value {attribute_volume}");

        let mut attributes = DocumentAttributes::default();
        attributes.insert("weight".into(), attribute_weight.clone());
        attributes.insert("mass".into(), attribute_mass.clone());

        // Resolve an attribute that is in the attributes map.
        let resolved = substitute("{weight}", HEADER, &attributes);
        assert_eq!(resolved, "weight".to_string());

        // Resolve two attributes that are in the attributes map.
        let resolved = substitute("{weight} {mass}", HEADER, &attributes);
        assert_eq!(resolved, "weight mass".to_string());

        // Resolve without attributes in the map
        let resolved = substitute("value {attribute_volume}", HEADER, &attributes);
        assert_eq!(resolved, attribute_volume_repeat);
    }

    #[test]
    fn test_utf8_boundary_handling() {
        // Regression test for fuzzer-found bug: UTF-8 multi-byte characters
        // should not cause panics during attribute substitution
        let attributes = DocumentAttributes::default();

        let values = [
            // Input with UTF-8 multi-byte character (Ô = 0xc3 0x94)
            ":J::~\x01\x00\x00Ô",
            // Test with various UTF-8 characters and attribute-like patterns
            "{attr}Ô{missing}日本語",
            // Test with multi-byte chars inside attribute name
            "{attrÔ}test",
        ];
        for value in values {
            let resolved = substitute(value, HEADER, &attributes);
            assert_eq!(resolved, value);
        }
    }
}
