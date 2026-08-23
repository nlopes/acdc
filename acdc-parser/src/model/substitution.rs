//! Substitution types and application for `AsciiDoc` content.
//!
//! # Parser and converter responsibilities
//!
//! Substitution handling depends on the content context. The parser resolves
//! named groups and applies substitutions that create inline structure. It also
//! records substitutions that require output-specific rendering on the AST.
//!
//! Inline passthroughs are processed in their requested order before the parser
//! returns the AST. Ordinary blocks retain their substitution specification so
//! converters can apply the block's output-specific behavior.
//!
//! Converters handle the format-specific parts:
//!
//! - **`SpecialChars`** - HTML converter escapes `<`, `>`, `&` to entities.
//!   Other converters may handle differently (e.g., terminal needs no escaping).
//!
//! - **Replacements** - Typography transformations (em-dashes, arrows, ellipsis).
//!   Output varies by format (HTML entities vs Unicode characters).
//!
//! Formatting, macros, line breaks, and callouts become structured AST nodes
//! when their substitution is active. Each converter decides how those nodes
//! appear in its output.

use std::borrow::Cow;

use serde::Serialize;

use crate::{AttributeValue, DocumentAttributes};

const SUBSTITUTION_STAGE_COUNT: usize = 7;
const DISABLED_SUBSTITUTION: u8 = u8::MAX;
const DEFAULT_PARSER_SUBSTITUTIONS: &[Substitution] = &[
    Substitution::SpecialChars,
    Substitution::Quotes,
    Substitution::Attributes,
    Substitution::Replacements,
    Substitution::Macros,
    Substitution::PostReplacements,
    Substitution::Callouts,
];

/// The enabled structural substitutions and their effective source order.
///
/// A disabled stage has the maximum rank. This keeps all ordering decisions in
/// one value instead of adding a Boolean for every pair of stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SubstitutionPlan {
    ranks: [u8; SUBSTITUTION_STAGE_COUNT],
}

impl SubstitutionPlan {
    pub(crate) fn from_substitutions(substitutions: &[Substitution]) -> Self {
        let mut ranks = [DISABLED_SUBSTITUTION; SUBSTITUTION_STAGE_COUNT];
        for (rank, substitution) in substitutions.iter().enumerate() {
            if let Some(slot) =
                substitution_stage_index(substitution).and_then(|index| ranks.get_mut(index))
            {
                *slot = u8::try_from(rank).unwrap_or(u8::MAX - 1);
            }
        }
        Self { ranks }
    }

    pub(crate) fn only(substitution: &Substitution) -> Self {
        Self::from_substitutions(std::slice::from_ref(substitution))
    }

    #[cfg(feature = "pre-spec-subs")]
    pub(crate) fn for_block_spec(spec: &SubstitutionSpec) -> Self {
        let mut substitutions = spec.resolve(NORMAL);
        if matches!(spec, SubstitutionSpec::Modifiers(_)) {
            for substitution in DEFAULT_PARSER_SUBSTITUTIONS {
                if !spec.is_disabled(substitution) && !substitutions.contains(substitution) {
                    substitutions.push(substitution.clone());
                }
            }
        }
        Self::from_substitutions(&substitutions)
    }

    pub(crate) fn enabled(self, substitution: &Substitution) -> bool {
        substitution_stage_index(substitution)
            .and_then(|index| self.ranks.get(index))
            .is_some_and(|rank| *rank != DISABLED_SUBSTITUTION)
    }

    pub(crate) fn precedes(self, first: &Substitution, second: &Substitution) -> bool {
        let (Some(first), Some(second)) = (
            substitution_stage_index(first),
            substitution_stage_index(second),
        ) else {
            return false;
        };
        let (Some(first), Some(second)) = (self.ranks.get(first), self.ranks.get(second)) else {
            return false;
        };
        *first != DISABLED_SUBSTITUTION && *second != DISABLED_SUBSTITUTION && first < second
    }
}

impl Default for SubstitutionPlan {
    fn default() -> Self {
        Self::from_substitutions(DEFAULT_PARSER_SUBSTITUTIONS)
    }
}

const fn substitution_stage_index(substitution: &Substitution) -> Option<usize> {
    match substitution {
        Substitution::SpecialChars => Some(0),
        Substitution::Attributes => Some(1),
        Substitution::Replacements => Some(2),
        Substitution::Macros => Some(3),
        Substitution::PostReplacements => Some(4),
        Substitution::Quotes => Some(5),
        Substitution::Callouts => Some(6),
        Substitution::Normal | Substitution::Verbatim => None,
    }
}

/// An `AsciiDoc` substitution or named substitution group.
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
        "specialchars" | "specialcharacters" | "c" => Some(Substitution::SpecialChars),
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
    Substitution::Quotes,
    Substitution::Attributes,
    Substitution::Replacements,
    Substitution::Macros,
    Substitution::PostReplacements,
];

/// Default substitutions for verbatim blocks (listing, literal).
pub const VERBATIM: &[Substitution] = &[Substitution::SpecialChars, Substitution::Callouts];

/// The inline `verbatim` group excludes block-only callout processing.
const PASSTHROUGH_VERBATIM: &[Substitution] = &[Substitution::SpecialChars];

#[derive(Clone, Copy)]
enum GroupContext {
    #[cfg(feature = "pre-spec-subs")]
    Block,
    Passthrough,
}

fn substitution_members(substitution: &Substitution, context: GroupContext) -> &[Substitution] {
    match (substitution, context) {
        (Substitution::Normal, _) => NORMAL,
        #[cfg(feature = "pre-spec-subs")]
        (Substitution::Verbatim, GroupContext::Block) => VERBATIM,
        (Substitution::Verbatim, GroupContext::Passthrough) => PASSTHROUGH_VERBATIM,
        (
            Substitution::SpecialChars
            | Substitution::Attributes
            | Substitution::Replacements
            | Substitution::Macros
            | Substitution::PostReplacements
            | Substitution::Quotes
            | Substitution::Callouts,
            _,
        ) => std::slice::from_ref(substitution),
    }
}

fn append_expanded_substitution(
    result: &mut Vec<Substitution>,
    substitution: &Substitution,
    context: GroupContext,
) {
    for member in substitution_members(substitution, context) {
        if !result.contains(member) {
            result.push(member.clone());
        }
    }
}

/// Resolve named groups for an inline passthrough while preserving source order.
pub(crate) fn resolve_passthrough_substitutions(
    substitutions: &[Substitution],
) -> Vec<Substitution> {
    let mut resolved = Vec::with_capacity(substitutions.len());
    for substitution in substitutions {
        append_expanded_substitution(&mut resolved, substitution, GroupContext::Passthrough);
    }
    resolved
}

/// A substitution operation to apply to a default substitution list.
///
/// Used when the `subs` attribute contains modifier syntax (`+quotes`, `-callouts`, `quotes+`).
///
/// Only available when the `pre-spec-subs` feature is enabled — the draft
/// `AsciiDoc` spec is dropping the substitution model in favour of an inline
/// parsing grammar, so this type goes away with the feature.
#[cfg(feature = "pre-spec-subs")]
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum SubstitutionOp {
    /// `+name` - append substitution to end of default list
    Append(Substitution),
    /// `name+` - prepend substitution to beginning of default list
    Prepend(Substitution),
    /// `-name` - remove substitution from default list
    Remove(Substitution),
}

#[cfg(feature = "pre-spec-subs")]
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
///
/// Only available when the `pre-spec-subs` feature is enabled.
#[cfg(feature = "pre-spec-subs")]
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum SubstitutionSpec {
    /// Explicit list of substitutions to apply (replaces all defaults)
    Explicit(Vec<Substitution>),
    /// Modifier operations to apply to block-type defaults
    Modifiers(Vec<SubstitutionOp>),
}

#[cfg(feature = "pre-spec-subs")]
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

#[cfg(feature = "pre-spec-subs")]
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

    fn is_disabled(&self, substitution: &Substitution) -> bool {
        match self {
            Self::Explicit(substitutions) => !substitutions.contains(substitution),
            Self::Modifiers(operations) => operations.iter().any(
                |operation| matches!(operation, SubstitutionOp::Remove(removed) if removed == substitution),
            ),
        }
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
#[cfg(feature = "pre-spec-subs")]
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
#[cfg(feature = "pre-spec-subs")]
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
#[cfg(feature = "pre-spec-subs")]
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

/// Append a substitution (or group) to the end of the list.
#[cfg(feature = "pre-spec-subs")]
pub(crate) fn append_substitution(result: &mut Vec<Substitution>, sub: &Substitution) {
    append_expanded_substitution(result, sub, GroupContext::Block);
}

/// Prepend a substitution (or group) to the beginning of the list.
#[cfg(feature = "pre-spec-subs")]
pub(crate) fn prepend_substitution(result: &mut Vec<Substitution>, sub: &Substitution) {
    // Insert in reverse order at position 0 to maintain group order
    for s in substitution_members(sub, GroupContext::Block).iter().rev() {
        if !result.contains(s) {
            result.insert(0, s.clone());
        }
    }
}

/// Remove a substitution (or group) from the list.
#[cfg(feature = "pre-spec-subs")]
pub(crate) fn remove_substitution(result: &mut Vec<Substitution>, sub: &Substitution) {
    for s in substitution_members(sub, GroupContext::Block) {
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
///   `PostReplacements`, `Callouts`) - No-op; handled by the converter
///   (`SpecialChars`, `Quotes`, `Replacements`) or by the grammar
///   (`Macros`, `PostReplacements`, `Callouts`).
///
/// # Example
///
/// ```
/// use acdc_parser::{DocumentAttributes, AttributeValue, Substitution, substitute};
///
/// let mut attrs = DocumentAttributes::default();
/// attrs.set("version".into(), AttributeValue::String("1.0".into()));
///
/// let result = substitute("Version {version}", &[Substitution::Attributes], &attrs);
/// assert_eq!(result, "Version 1.0");
/// ```
#[must_use]
pub fn substitute<'a, 'b>(
    text: &'b str,
    substitutions: &[Substitution],
    attributes: &DocumentAttributes<'a>,
) -> Cow<'b, str>
where
    'a: 'b,
{
    let mut result = Cow::Borrowed(text);
    for substitution in substitutions {
        match substitution {
            Substitution::Attributes => {
                // Expand {name} patterns with values from document attributes
                if !result.contains('{') {
                    continue;
                }

                let mut expanded = String::with_capacity(result.len());
                let mut chars = result.chars().peekable();
                let mut changed = false;

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
                                    changed = true;
                                }
                                Some(AttributeValue::String(attr_value)) => {
                                    expanded.push_str(attr_value);
                                    changed = true;
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
                if changed {
                    result = Cow::Owned(expanded);
                }
            }
            // These substitutions are handled elsewhere — the converter
            // (`SpecialChars`, `Quotes`, `Replacements`) or the grammar
            // (`Macros`, `PostReplacements`, `Callouts`). They are no-ops
            // here in `substitute()`.
            Substitution::SpecialChars
            | Substitution::Quotes
            | Substitution::Replacements
            | Substitution::Macros
            | Substitution::PostReplacements
            | Substitution::Callouts => {}
            // Group substitutions expand recursively
            Substitution::Normal => {
                let current = std::mem::take(&mut result);
                result = match current {
                    Cow::Borrowed(s) => substitute(s, NORMAL, attributes),
                    Cow::Owned(s) => Cow::Owned(substitute(&s, NORMAL, attributes).into_owned()),
                };
            }
            Substitution::Verbatim => {
                let current = std::mem::take(&mut result);
                result = match current {
                    Cow::Borrowed(s) => substitute(s, VERBATIM, attributes),
                    Cow::Owned(s) => Cow::Owned(substitute(&s, VERBATIM, attributes).into_owned()),
                };
            }
        }
    }
    result
}

#[cfg(test)]
mod group_tests {
    use super::*;

    #[test]
    fn normal_group_uses_reference_order() {
        assert_eq!(
            NORMAL,
            &[
                Substitution::SpecialChars,
                Substitution::Quotes,
                Substitution::Attributes,
                Substitution::Replacements,
                Substitution::Macros,
                Substitution::PostReplacements,
            ]
        );
    }

    #[test]
    fn passthrough_groups_use_inline_policies() {
        assert_eq!(
            resolve_passthrough_substitutions(&[Substitution::Normal]),
            NORMAL
        );
        assert_eq!(
            resolve_passthrough_substitutions(&[Substitution::Verbatim]),
            [Substitution::SpecialChars]
        );
    }

    #[test]
    fn passthrough_group_expansion_preserves_first_occurrence() {
        assert_eq!(
            resolve_passthrough_substitutions(&[
                Substitution::Attributes,
                Substitution::Normal,
                Substitution::Quotes,
            ]),
            [
                Substitution::Attributes,
                Substitution::SpecialChars,
                Substitution::Quotes,
                Substitution::Replacements,
                Substitution::Macros,
                Substitution::PostReplacements,
            ]
        );
    }
}

// Tests cover the `subs=` machinery (parse_subs_attribute, SubstitutionSpec,
// SubstitutionOp), all of which are feature-gated. `substitute()` is
// exercised indirectly through the parser's fixture suite.
#[cfg(all(test, feature = "pre-spec-subs"))]
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
    fn test_parse_subs_specialcharacters_alias() {
        let result = parse_subs_attribute("specialcharacters");
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
        let attribute_weight: AttributeValue = "weight".into();
        let attribute_mass: AttributeValue = "mass".into();

        // This one is an attribute we do NOT add to the attributes map so it can never be
        // resolved.
        let attribute_volume_repeat = "value {attribute_volume}";

        let mut attributes = DocumentAttributes::default();
        attributes.insert("weight".into(), attribute_weight.clone());
        attributes.insert("mass".into(), attribute_mass.clone());

        // Resolve an attribute that is in the attributes map.
        let resolved = substitute("{weight}", HEADER, &attributes);
        assert_eq!(resolved, "weight");

        // Resolve two attributes that are in the attributes map.
        let resolved = substitute("{weight} {mass}", HEADER, &attributes);
        assert_eq!(resolved, "weight mass");

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
        attributes.insert("foo".into(), "{bar}".into());
        attributes.insert("bar".into(), "should-not-appear".into());

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

    // One row per `subs=` attribute × substitution to inspect. Covers the four
    // forms accepted by `parse_subs_attribute` (explicit list, single short
    // alias, modifier list, the special `none` keyword) against each known
    // substitution. `expected = true` means the substitution is disabled by
    // that spec.
    #[rstest::rstest]
    // Explicit list — `specialchars` disables everything else.
    #[case::explicit_specialchars_disables_macros("specialchars", Substitution::Macros, true)]
    #[case::explicit_specialchars_disables_attributes(
        "specialchars",
        Substitution::Attributes,
        true
    )]
    #[case::explicit_specialchars_disables_post_replacements(
        "specialchars",
        Substitution::PostReplacements,
        true
    )]
    #[case::explicit_specialchars_disables_quotes("specialchars", Substitution::Quotes, true)]
    #[case::explicit_specialchars_disables_callouts("specialchars", Substitution::Callouts, true)]
    // Explicit single-name list — that one substitution is enabled.
    #[case::explicit_macros("macros", Substitution::Macros, false)]
    #[case::explicit_attributes("attributes", Substitution::Attributes, false)]
    #[case::explicit_post_replacements("post_replacements", Substitution::PostReplacements, false)]
    #[case::explicit_quotes("quotes", Substitution::Quotes, false)]
    #[case::explicit_callouts("callouts", Substitution::Callouts, false)]
    // Short aliases resolve to the same enabled state.
    #[case::short_alias_p_enables_post_replacements("p", Substitution::PostReplacements, false)]
    #[case::short_alias_q_enables_quotes("q", Substitution::Quotes, false)]
    // Baseline groups (`normal` / `verbatim`) include their members.
    #[case::baseline_normal_includes_macros("normal", Substitution::Macros, false)]
    #[case::baseline_normal_includes_attributes("normal", Substitution::Attributes, false)]
    #[case::baseline_normal_includes_post_replacements(
        "normal",
        Substitution::PostReplacements,
        false
    )]
    #[case::baseline_normal_includes_quotes("normal", Substitution::Quotes, false)]
    #[case::baseline_verbatim_includes_callouts("verbatim", Substitution::Callouts, false)]
    // Modifier remove `-X` disables only X.
    #[case::modifier_remove_macros("-macros", Substitution::Macros, true)]
    #[case::modifier_remove_attributes("-attributes", Substitution::Attributes, true)]
    #[case::modifier_remove_post_replacements(
        "-post_replacements",
        Substitution::PostReplacements,
        true
    )]
    #[case::modifier_remove_quotes("-quotes", Substitution::Quotes, true)]
    #[case::modifier_remove_callouts("-callouts", Substitution::Callouts, true)]
    // Modifier add `+X` leaves X enabled (no `Remove` op present).
    #[case::modifier_add_macros("+macros", Substitution::Macros, false)]
    #[case::modifier_add_attributes("+attributes", Substitution::Attributes, false)]
    #[case::modifier_add_post_replacements(
        "+post_replacements",
        Substitution::PostReplacements,
        false
    )]
    #[case::modifier_add_quotes("+quotes", Substitution::Quotes, false)]
    #[case::modifier_add_callouts("+callouts", Substitution::Callouts, false)]
    // `none` is the explicit empty list and disables everything.
    #[case::none_disables_macros("none", Substitution::Macros, true)]
    #[case::none_disables_attributes("none", Substitution::Attributes, true)]
    #[case::none_disables_post_replacements("none", Substitution::PostReplacements, true)]
    #[case::none_disables_quotes("none", Substitution::Quotes, true)]
    #[case::none_disables_callouts("none", Substitution::Callouts, true)]
    fn is_disabled_matches_spec(
        #[case] subs_attr: &str,
        #[case] sub: Substitution,
        #[case] expected: bool,
    ) {
        let spec = parse_subs_attribute(subs_attr);
        assert_eq!(
            spec.is_disabled(&sub),
            expected,
            "spec={subs_attr:?} sub={sub:?}"
        );
    }
}
