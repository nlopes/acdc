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

use serde::Serialize;

use crate::{AttributeValue, DocumentAttributes};

/// A `Substitution` represents a substitution in a passthrough macro.
#[derive(Clone, Debug, Hash, Eq, PartialEq, Serialize)]
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

impl std::fmt::Display for Substitution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::SpecialChars => "special_chars",
            Self::Attributes => "attributes",
            Self::Replacements => "replacements",
            Self::Macros => "macros",
            Self::PostReplacements => "post_replacements",
            Self::Normal => "normal",
            Self::Verbatim => "verbatim",
            Self::Quotes => "quotes",
            Self::Callouts => "callouts",
        };
        write!(f, "{name}")
    }
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

/// A substitution operation to apply to a default substitution list.
///
/// Used when the `subs` attribute contains modifier syntax (`+quotes`, `-callouts`, `quotes+`).
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum SubstitutionOp {
    /// `+name` - append substitution to end of default list
    Append(Substitution),
    /// `name+` - prepend substitution to beginning of default list
    Prepend(Substitution),
    /// `-name` - remove substitution from default list
    Remove(Substitution),
}

impl std::fmt::Display for SubstitutionOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Append(sub) => write!(f, "+{sub}"),
            Self::Prepend(sub) => write!(f, "{sub}+"),
            Self::Remove(sub) => write!(f, "-{sub}"),
        }
    }
}

/// Specification for substitutions to apply to a block.
///
/// This type represents how substitutions are specified in a `subs` attribute:
///
/// - **Explicit**: A direct list of substitutions (e.g., `subs=specialchars,quotes`)
/// - **Modifiers**: Operations to apply to the block-type default substitutions
///   (e.g., `subs=+quotes,-callouts`)
///
/// The parser cannot know the block type when parsing attributes (metadata comes before
/// the block delimiter), so modifier operations are stored and the converter applies
/// them with the appropriate baseline (VERBATIM for listing/literal, NORMAL for paragraphs).
///
/// ## Serialization
///
/// Serializes to a flat array of strings matching document syntax:
/// - Explicit: `["special_chars", "quotes"]`
/// - Modifiers: `["+quotes", "-callouts", "macros+"]`
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum SubstitutionSpec {
    /// Explicit list of substitutions to apply (replaces all defaults)
    Explicit(Vec<Substitution>),
    /// Modifier operations to apply to block-type defaults
    Modifiers(Vec<SubstitutionOp>),
}

impl Serialize for SubstitutionSpec {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let strings: Vec<String> = match self {
            Self::Explicit(subs) => subs.iter().map(ToString::to_string).collect(),
            Self::Modifiers(ops) => ops.iter().map(ToString::to_string).collect(),
        };
        strings.serialize(serializer)
    }
}

impl SubstitutionSpec {
    /// Apply modifier operations to a default substitution list.
    ///
    /// This is used by converters to resolve modifiers with the appropriate baseline.
    #[must_use]
    pub fn apply_modifiers(ops: &[SubstitutionOp], default: &[Substitution]) -> Vec<Substitution> {
        let mut result = default.to_vec();
        for op in ops {
            match op {
                SubstitutionOp::Append(sub) => append_substitution(&mut result, sub),
                SubstitutionOp::Prepend(sub) => prepend_substitution(&mut result, sub),
                SubstitutionOp::Remove(sub) => remove_substitution(&mut result, sub),
            }
        }
        result
    }

    /// Resolve the substitution spec to a concrete list of substitutions.
    ///
    /// - For `Explicit`, returns the list directly
    /// - For `Modifiers`, applies the operations to the provided default
    #[must_use]
    pub fn resolve(&self, default: &[Substitution]) -> Vec<Substitution> {
        match self {
            SubstitutionSpec::Explicit(subs) => subs.clone(),
            SubstitutionSpec::Modifiers(ops) => Self::apply_modifiers(ops, default),
        }
    }
}

/// Modifier for a substitution in the `subs` attribute (internal parsing helper).
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

/// Parse a `subs` attribute value into a substitution specification.
///
/// Returns either:
/// - `SubstitutionSpec::Explicit` for explicit lists (e.g., `subs=specialchars,quotes`)
/// - `SubstitutionSpec::Modifiers` for modifier syntax (e.g., `subs=+quotes,-callouts`)
///
/// Supports:
/// - `none` → Explicit empty list (no substitutions)
/// - `normal` → Explicit NORMAL list
/// - `verbatim` → Explicit VERBATIM list
/// - `a,q,c` → Explicit specific substitutions (comma-separated)
/// - `+quotes` → Modifiers: append to end of default list
/// - `quotes+` → Modifiers: prepend to beginning of default list
/// - `-specialchars` → Modifiers: remove from default list
/// - `specialchars,+quotes` → Modifiers: mixed modifier mode
///
/// Order matters: substitutions/modifiers are applied in sequence.
#[must_use]
pub(crate) fn parse_subs_attribute(value: &str) -> SubstitutionSpec {
    let value = value.trim();

    // Handle special cases
    if value.is_empty() || value == "none" {
        return SubstitutionSpec::Explicit(Vec::new());
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
        // Modifier mode: collect operations for converter to apply
        let mut ops = Vec::new();

        for (name, modifier) in parts {
            // Parse the substitution name; skip if invalid
            let Some(sub) = parse_substitution(name) else {
                continue;
            };

            match modifier {
                Some(SubsModifier::Append) => {
                    ops.push(SubstitutionOp::Append(sub));
                }
                Some(SubsModifier::Prepend) => {
                    ops.push(SubstitutionOp::Prepend(sub));
                }
                Some(SubsModifier::Remove) => {
                    ops.push(SubstitutionOp::Remove(sub));
                }
                None => {
                    // Plain substitution name in modifier context - warn and treat as append
                    tracing::warn!(
                        substitution = %name,
                        "plain substitution in modifier context; consider +{name} for clarity"
                    );
                    ops.push(SubstitutionOp::Append(sub));
                }
            }
        }
        SubstitutionSpec::Modifiers(ops)
    } else {
        // No modifiers - parse as an explicit list of substitution names (in order)
        let mut result = Vec::new();
        for (name, _) in parts {
            if let Some(ref sub) = parse_substitution(name) {
                append_substitution(&mut result, sub);
            }
        }
        SubstitutionSpec::Explicit(result)
    }
}

/// Expand a substitution to its constituent list.
///
/// Groups (`Normal`, `Verbatim`) expand to their members; individual subs return themselves.
fn expand_substitution(sub: &Substitution) -> &[Substitution] {
    match sub {
        Substitution::Normal => NORMAL,
        Substitution::Verbatim => VERBATIM,
        Substitution::SpecialChars
        | Substitution::Attributes
        | Substitution::Replacements
        | Substitution::Macros
        | Substitution::PostReplacements
        | Substitution::Quotes
        | Substitution::Callouts => std::slice::from_ref(sub),
    }
}

/// Append a substitution (or group) to the end of the list.
pub(crate) fn append_substitution(result: &mut Vec<Substitution>, sub: &Substitution) {
    for s in expand_substitution(sub) {
        if !result.contains(s) {
            result.push(s.clone());
        }
    }
}

/// Prepend a substitution (or group) to the beginning of the list.
pub(crate) fn prepend_substitution(result: &mut Vec<Substitution>, sub: &Substitution) {
    // Insert in reverse order at position 0 to maintain group order
    for s in expand_substitution(sub).iter().rev() {
        if !result.contains(s) {
            result.insert(0, s.clone());
        }
    }
}

/// Remove a substitution (or group) from the list.
pub(crate) fn remove_substitution(result: &mut Vec<Substitution>, sub: &Substitution) {
    for s in expand_substitution(sub) {
        result.retain(|x| x != s);
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

    // Helper to extract explicit list from SubstitutionSpec
    #[allow(clippy::panic)]
    fn explicit(spec: &SubstitutionSpec) -> &Vec<Substitution> {
        match spec {
            SubstitutionSpec::Explicit(subs) => subs,
            SubstitutionSpec::Modifiers(_) => panic!("Expected Explicit, got Modifiers"),
        }
    }

    // Helper to extract modifiers from SubstitutionSpec
    #[allow(clippy::panic)]
    fn modifiers(spec: &SubstitutionSpec) -> &Vec<SubstitutionOp> {
        match spec {
            SubstitutionSpec::Modifiers(ops) => ops,
            SubstitutionSpec::Explicit(_) => panic!("Expected Modifiers, got Explicit"),
        }
    }

    #[test]
    fn test_parse_subs_none() {
        let result = parse_subs_attribute("none");
        assert!(explicit(&result).is_empty());
    }

    #[test]
    fn test_parse_subs_empty_string() {
        let result = parse_subs_attribute("");
        assert!(explicit(&result).is_empty());
    }

    #[test]
    fn test_parse_subs_none_with_whitespace() {
        let result = parse_subs_attribute("  none  ");
        assert!(explicit(&result).is_empty());
    }

    #[test]
    fn test_parse_subs_specialchars() {
        let result = parse_subs_attribute("specialchars");
        assert_eq!(explicit(&result), &vec![Substitution::SpecialChars]);
    }

    #[test]
    fn test_parse_subs_specialchars_shorthand() {
        let result = parse_subs_attribute("c");
        assert_eq!(explicit(&result), &vec![Substitution::SpecialChars]);
    }

    #[test]
    fn test_parse_subs_normal_expands() {
        let result = parse_subs_attribute("normal");
        assert_eq!(explicit(&result), &NORMAL.to_vec());
    }

    #[test]
    fn test_parse_subs_verbatim_expands() {
        let result = parse_subs_attribute("verbatim");
        assert_eq!(explicit(&result), &VERBATIM.to_vec());
    }

    #[test]
    fn test_parse_subs_append_modifier() {
        let result = parse_subs_attribute("+quotes");
        let ops = modifiers(&result);
        assert_eq!(ops, &vec![SubstitutionOp::Append(Substitution::Quotes)]);

        // Verify resolved result with VERBATIM baseline
        let resolved = result.resolve(VERBATIM);
        assert!(resolved.contains(&Substitution::SpecialChars));
        assert!(resolved.contains(&Substitution::Callouts));
        assert!(resolved.contains(&Substitution::Quotes));
        assert_eq!(resolved.last(), Some(&Substitution::Quotes));
    }

    #[test]
    fn test_parse_subs_prepend_modifier() {
        let result = parse_subs_attribute("quotes+");
        let ops = modifiers(&result);
        assert_eq!(ops, &vec![SubstitutionOp::Prepend(Substitution::Quotes)]);

        // Verify resolved result with VERBATIM baseline
        let resolved = result.resolve(VERBATIM);
        assert_eq!(resolved.first(), Some(&Substitution::Quotes));
        assert!(resolved.contains(&Substitution::SpecialChars));
        assert!(resolved.contains(&Substitution::Callouts));
    }

    #[test]
    fn test_parse_subs_remove_modifier() {
        let result = parse_subs_attribute("-specialchars");
        let ops = modifiers(&result);
        assert_eq!(
            ops,
            &vec![SubstitutionOp::Remove(Substitution::SpecialChars)]
        );

        // Verify resolved result with VERBATIM baseline
        let resolved = result.resolve(VERBATIM);
        assert!(!resolved.contains(&Substitution::SpecialChars));
        assert!(resolved.contains(&Substitution::Callouts));
    }

    #[test]
    fn test_parse_subs_remove_all_verbatim() {
        let result = parse_subs_attribute("-specialchars,-callouts");
        let ops = modifiers(&result);
        assert_eq!(ops.len(), 2);

        // Verify resolved result with VERBATIM baseline
        let resolved = result.resolve(VERBATIM);
        assert!(resolved.is_empty());
    }

    #[test]
    fn test_parse_subs_combined_modifiers() {
        let result = parse_subs_attribute("+quotes,-callouts");
        let ops = modifiers(&result);
        assert_eq!(ops.len(), 2);

        // Verify resolved result with VERBATIM baseline
        let resolved = result.resolve(VERBATIM);
        assert!(resolved.contains(&Substitution::SpecialChars)); // from default
        assert!(resolved.contains(&Substitution::Quotes)); // added
        assert!(!resolved.contains(&Substitution::Callouts)); // removed
    }

    #[test]
    fn test_parse_subs_ordering_preserved() {
        let result = parse_subs_attribute("quotes,attributes,specialchars");
        assert_eq!(
            explicit(&result),
            &vec![
                Substitution::Quotes,
                Substitution::Attributes,
                Substitution::SpecialChars
            ]
        );
    }

    #[test]
    fn test_parse_subs_shorthand_list() {
        let result = parse_subs_attribute("q,a,c");
        assert_eq!(
            explicit(&result),
            &vec![
                Substitution::Quotes,
                Substitution::Attributes,
                Substitution::SpecialChars
            ]
        );
    }

    #[test]
    fn test_parse_subs_with_spaces() {
        let result = parse_subs_attribute(" quotes , attributes ");
        assert_eq!(
            explicit(&result),
            &vec![Substitution::Quotes, Substitution::Attributes]
        );
    }

    #[test]
    fn test_parse_subs_duplicates_ignored() {
        let result = parse_subs_attribute("quotes,quotes,quotes");
        assert_eq!(explicit(&result), &vec![Substitution::Quotes]);
    }

    #[test]
    fn test_parse_subs_normal_in_list_expands() {
        let result = parse_subs_attribute("normal");
        let subs = explicit(&result);
        // Should expand to all NORMAL substitutions
        assert_eq!(subs.len(), NORMAL.len());
        for sub in NORMAL {
            assert!(subs.contains(sub));
        }
    }

    #[test]
    fn test_parse_subs_append_normal_group() {
        let result = parse_subs_attribute("+normal");
        // This is modifier syntax, resolve with a baseline that has Callouts
        let resolved = result.resolve(&[Substitution::Callouts]);
        // Should have Callouts + all of NORMAL
        assert!(resolved.contains(&Substitution::Callouts));
        for sub in NORMAL {
            assert!(resolved.contains(sub));
        }
    }

    #[test]
    fn test_parse_subs_remove_normal_group() {
        let result = parse_subs_attribute("-normal");
        // This is modifier syntax, resolve with NORMAL baseline
        let resolved = result.resolve(NORMAL);
        // Removing normal group should leave empty
        assert!(resolved.is_empty());
    }

    #[test]
    fn test_parse_subs_unknown_is_skipped() {
        // Unknown substitution types are logged and skipped
        let result = parse_subs_attribute("unknown");
        assert!(explicit(&result).is_empty());
    }

    #[test]
    fn test_parse_subs_unknown_mixed_with_valid() {
        // Unknown substitution types are skipped, valid ones are kept
        let result = parse_subs_attribute("quotes,typo,attributes");
        assert_eq!(
            explicit(&result),
            &vec![Substitution::Quotes, Substitution::Attributes]
        );
    }

    #[test]
    fn test_parse_subs_all_individual_types() {
        // Test each substitution type can be parsed
        assert_eq!(
            explicit(&parse_subs_attribute("attributes")),
            &vec![Substitution::Attributes]
        );
        assert_eq!(
            explicit(&parse_subs_attribute("replacements")),
            &vec![Substitution::Replacements]
        );
        assert_eq!(
            explicit(&parse_subs_attribute("macros")),
            &vec![Substitution::Macros]
        );
        assert_eq!(
            explicit(&parse_subs_attribute("post_replacements")),
            &vec![Substitution::PostReplacements]
        );
        assert_eq!(
            explicit(&parse_subs_attribute("quotes")),
            &vec![Substitution::Quotes]
        );
        assert_eq!(
            explicit(&parse_subs_attribute("callouts")),
            &vec![Substitution::Callouts]
        );
    }

    #[test]
    fn test_parse_subs_shorthand_types() {
        assert_eq!(
            explicit(&parse_subs_attribute("a")),
            &vec![Substitution::Attributes]
        );
        assert_eq!(
            explicit(&parse_subs_attribute("r")),
            &vec![Substitution::Replacements]
        );
        assert_eq!(
            explicit(&parse_subs_attribute("m")),
            &vec![Substitution::Macros]
        );
        assert_eq!(
            explicit(&parse_subs_attribute("p")),
            &vec![Substitution::PostReplacements]
        );
        assert_eq!(
            explicit(&parse_subs_attribute("q")),
            &vec![Substitution::Quotes]
        );
        assert_eq!(
            explicit(&parse_subs_attribute("c")),
            &vec![Substitution::SpecialChars]
        );
    }

    #[test]
    fn test_parse_subs_mixed_modifier_list() {
        // Bug case: subs=specialchars,+quotes - modifier not at start of string
        let result = parse_subs_attribute("specialchars,+quotes");
        // Should be in modifier mode
        let ops = modifiers(&result);
        assert_eq!(ops.len(), 2); // specialchars (as append) and +quotes

        // Verify resolved result with VERBATIM baseline
        let resolved = result.resolve(VERBATIM);
        assert!(resolved.contains(&Substitution::SpecialChars));
        assert!(resolved.contains(&Substitution::Callouts)); // from VERBATIM default
        assert!(resolved.contains(&Substitution::Quotes)); // appended
    }

    #[test]
    fn test_parse_subs_modifier_in_middle() {
        // subs=attributes,+quotes,-callouts
        let result = parse_subs_attribute("attributes,+quotes,-callouts");
        let ops = modifiers(&result);
        assert_eq!(ops.len(), 3);

        // Verify resolved result with VERBATIM baseline
        let resolved = result.resolve(VERBATIM);
        assert!(resolved.contains(&Substitution::Attributes)); // plain name in modifier context
        assert!(resolved.contains(&Substitution::Quotes)); // appended
        assert!(!resolved.contains(&Substitution::Callouts)); // removed
    }

    #[test]
    fn test_parse_subs_asciidoctor_example() {
        // From asciidoctor docs: subs="attributes+,+replacements,-callouts"
        let result = parse_subs_attribute("attributes+,+replacements,-callouts");
        let ops = modifiers(&result);
        assert_eq!(ops.len(), 3);

        // Verify resolved result with VERBATIM baseline
        let resolved = result.resolve(VERBATIM);
        assert_eq!(resolved.first(), Some(&Substitution::Attributes)); // prepended
        assert!(resolved.contains(&Substitution::Replacements)); // appended
        assert!(!resolved.contains(&Substitution::Callouts)); // removed
    }

    #[test]
    fn test_parse_subs_modifier_only_at_end() {
        // Modifier at end of comma-separated list
        let result = parse_subs_attribute("quotes,-specialchars");
        // Should detect modifier mode from -specialchars
        let ops = modifiers(&result);
        assert_eq!(ops.len(), 2);

        // Verify resolved result with VERBATIM baseline
        let resolved = result.resolve(VERBATIM);
        assert!(resolved.contains(&Substitution::Quotes)); // plain name appended
        assert!(!resolved.contains(&Substitution::SpecialChars)); // removed
        assert!(resolved.contains(&Substitution::Callouts)); // from default
    }

    #[test]
    fn test_resolve_modifiers_with_normal_baseline() {
        // This is the key test for the bug fix:
        // -quotes on a paragraph should remove quotes from NORMAL baseline
        let result = parse_subs_attribute("-quotes");
        let resolved = result.resolve(NORMAL);

        // Should have all of NORMAL except Quotes
        assert!(resolved.contains(&Substitution::SpecialChars));
        assert!(resolved.contains(&Substitution::Attributes));
        assert!(!resolved.contains(&Substitution::Quotes)); // removed
        assert!(resolved.contains(&Substitution::Replacements));
        assert!(resolved.contains(&Substitution::Macros));
        assert!(resolved.contains(&Substitution::PostReplacements));
    }

    #[test]
    fn test_resolve_modifiers_with_verbatim_baseline() {
        // -quotes on a listing block: Quotes wasn't in VERBATIM, so no effect
        let result = parse_subs_attribute("-quotes");
        let resolved = result.resolve(VERBATIM);

        // Should still have all of VERBATIM (quotes wasn't there to remove)
        assert!(resolved.contains(&Substitution::SpecialChars));
        assert!(resolved.contains(&Substitution::Callouts));
        assert!(!resolved.contains(&Substitution::Quotes));
    }

    #[test]
    fn test_resolve_explicit_ignores_baseline() {
        // Explicit lists should ignore the baseline
        let result = parse_subs_attribute("quotes,attributes");
        let resolved_normal = result.resolve(NORMAL);
        let resolved_verbatim = result.resolve(VERBATIM);

        // Both should be the same
        assert_eq!(resolved_normal, resolved_verbatim);
        assert_eq!(
            resolved_normal,
            vec![Substitution::Quotes, Substitution::Attributes]
        );
    }

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
    fn test_substitute_single_pass_expansion() {
        // Test that the substitute() function does single-pass expansion.
        // When foo's value is "{bar}", substitute("{foo}") returns the literal
        // "{bar}" string - it does NOT recursively resolve {bar}.
        //
        // This is correct behavior because:
        // 1. Definition-time resolution is handled separately (in the grammar parser)
        // 2. The substitute function just replaces one level of references
        let mut attributes = DocumentAttributes::default();
        attributes.insert("foo".into(), AttributeValue::String("{bar}".to_string()));
        attributes.insert(
            "bar".into(),
            AttributeValue::String("should-not-appear".to_string()),
        );

        let resolved = substitute("{foo}", HEADER, &attributes);
        assert_eq!(resolved, "{bar}");
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
