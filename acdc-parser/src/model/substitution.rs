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

use crate::DocumentAttributes;

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
        // Parser defaults include callout discovery for verbatim blocks. Resolve once:
        // filling missing stages afterwards would restore removed group members.
        Self::from_substitutions(&spec.resolve(DEFAULT_PARSER_SUBSTITUTIONS))
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

    pub(crate) fn through(mut self, substitution: &Substitution) -> Self {
        if let Some(last) = substitution_stage_index(substitution)
            .and_then(|index| self.ranks.get(index))
            .copied()
        {
            for rank in &mut self.ranks {
                if *rank > last {
                    *rank = DISABLED_SUBSTITUTION;
                }
            }
        }
        self
    }

    pub(crate) fn after(self, substitution: &Substitution) -> Vec<Substitution> {
        let mut stages: Vec<_> = DEFAULT_PARSER_SUBSTITUTIONS
            .iter()
            .filter(|stage| self.precedes(substitution, stage))
            .cloned()
            .collect();
        stages.sort_by_key(|stage| {
            substitution_stage_index(stage)
                .and_then(|index| self.ranks.get(index))
                .copied()
        });
        stages
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
    let substitution = substitution_named(value);
    if substitution.is_none() {
        tracing::error!(
            substitution = %value,
            "unknown substitution type, ignoring - check for typos"
        );
    }
    substitution
}

fn substitution_named(value: &str) -> Option<Substitution> {
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
        _ => None,
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
/// Parsed documents use [`Self::Source`] to preserve the entries in the `subs`
/// attribute, including groups, aliases, duplicates and modifiers. Only whitespace
/// around each entry is trimmed. Attribute references in the list have already
/// been expanded by metadata parsing.
///
/// Standalone `none` and empty values use [`Self::Explicit`] with an empty list.
/// They disable substitutions, whereas absent metadata uses block defaults.
/// Mixed lists retain `none` because its position can select the starting list.
///
/// [`Self::resolve`] derives the effective stages without changing the stored
/// entries. A plain first entry starts an empty list; a modifier first entry
/// starts with the supplied block defaults. Later entries operate on that list.
/// [`Self::Explicit`] and [`Self::Modifiers`] also support typed construction.
///
/// ## Serialization
///
/// Serializes to a flat array. Source entries retain their spelling, for example
/// `["verbatim", "-macros"]`; typed entries use canonical names such as
/// `"special_chars"`.
///
/// Only available when the `pre-spec-subs` feature is enabled.
#[cfg(feature = "pre-spec-subs")]
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum SubstitutionSpec {
    /// Authored entries, interpreted when resolving against block defaults.
    Source(Vec<String>),
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
            Self::Source(entries) => return entries.serialize(serializer),
            Self::Explicit(subs) => subs.iter().map(ToString::to_string).collect(),
            Self::Modifiers(ops) => ops.iter().map(ToString::to_string).collect(),
        };
        strings.serialize(serializer)
    }
}

#[cfg(feature = "pre-spec-subs")]
impl SubstitutionSpec {
    pub(crate) fn contains(&self, substitution: &Substitution, defaults: &[Substitution]) -> bool {
        match self {
            Self::Source(entries) => entries
                .iter()
                .rev()
                .find_map(|entry| {
                    let (name, modifier) = parse_subs_part(entry.trim());
                    let affected = substitution_named(name)?;
                    substitution_members(&affected, GroupContext::Block)
                        .contains(substitution)
                        .then_some(!matches!(modifier, Some(SubsModifier::Remove)))
                })
                .unwrap_or_else(|| {
                    entries
                        .first()
                        .is_some_and(|entry| parse_subs_part(entry.trim()).1.is_some())
                        && defaults.contains(substitution)
                }),
            Self::Explicit(substitutions) => substitutions.contains(substitution),
            // The last operation affecting a substitution determines its membership.
            Self::Modifiers(operations) => operations
                .iter()
                .rev()
                .find_map(|operation| {
                    let (affected, enabled) = match operation {
                        SubstitutionOp::Append(affected) | SubstitutionOp::Prepend(affected) => {
                            (affected, true)
                        }
                        SubstitutionOp::Remove(affected) => (affected, false),
                    };
                    substitution_members(affected, GroupContext::Block)
                        .contains(substitution)
                        .then_some(enabled)
                })
                .unwrap_or_else(|| defaults.contains(substitution)),
        }
    }

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
    /// - For `Source`, applies its entries in order, expanding named groups
    /// - For `Explicit`, returns the list directly
    /// - For `Modifiers`, applies the operations to the provided default
    #[must_use]
    pub fn resolve(&self, default: &[Substitution]) -> Vec<Substitution> {
        match self {
            SubstitutionSpec::Source(entries) => resolve_source_substitutions(entries, default),
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

/// Preserve `subs` entries and report unknown names once during parsing.
#[cfg(feature = "pre-spec-subs")]
#[must_use]
pub(crate) fn parse_subs_attribute(value: &str) -> SubstitutionSpec {
    let value = value.trim();
    if value.is_empty() || value == "none" {
        return SubstitutionSpec::Explicit(Vec::new());
    }
    let entries = value
        .split(',')
        .map(|entry| entry.trim().to_owned())
        .collect::<Vec<_>>();
    for entry in &entries {
        let (name, _) = parse_subs_part(entry);
        if !name.is_empty() && name != "none" {
            let _ = parse_substitution(name);
        }
    }
    SubstitutionSpec::Source(entries)
}

#[cfg(feature = "pre-spec-subs")]
fn resolve_source_substitutions(entries: &[String], default: &[Substitution]) -> Vec<Substitution> {
    let mut parts = entries
        .iter()
        .map(|entry| parse_subs_part(entry.trim()))
        .peekable();

    // The first entry selects the baseline, even if its name is empty or unknown.
    let uses_defaults = parts.peek().is_some_and(|(_, modifier)| modifier.is_some());
    let mut result = if uses_defaults {
        default.to_vec()
    } else {
        Vec::new()
    };
    for (name, modifier) in parts {
        // Invalid names remain in the AST; resolution must not repeat parse diagnostics.
        let Some(substitution) = substitution_named(name) else {
            continue;
        };
        match modifier {
            Some(SubsModifier::Prepend) => prepend_substitution(&mut result, &substitution),
            Some(SubsModifier::Remove) => remove_substitution(&mut result, &substitution),
            Some(SubsModifier::Append) | None => append_substitution(&mut result, &substitution),
        }
    }
    result
}

/// Append a substitution (or group) to the end of the list.
#[cfg(feature = "pre-spec-subs")]
pub(crate) fn append_substitution(result: &mut Vec<Substitution>, sub: &Substitution) {
    append_expanded_substitution(result, sub, GroupContext::Block);
}

/// Move a substitution (or group) to the beginning, preserving group order.
#[cfg(feature = "pre-spec-subs")]
pub(crate) fn prepend_substitution(result: &mut Vec<Substitution>, sub: &Substitution) {
    // The prepended occurrence wins, including members already enabled by the baseline.
    remove_substitution(result, sub);
    let members = substitution_members(sub, GroupContext::Block);
    result.splice(0..0, members.iter().cloned());
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
/// use acdc_parser::{Options, Substitution, substitute};
///
/// let options = Options::with_attributes([("version", "1.0")])?;
/// let attrs = options.document_attributes();
///
/// let result = substitute("Version {version}", &[Substitution::Attributes], attrs);
/// assert_eq!(result, "Version 1.0");
/// # Ok::<(), acdc_parser::Error>(())
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
                if let Cow::Owned(expanded) =
                    substitute_attributes(&result, |name| attributes.get(name))
                {
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
    use crate::AttributeValue;

    proptest::proptest! {
        #[test]
        fn membership_matches_resolved_modifier_order(
            operations in proptest::collection::vec((0_usize..9, 0_u8..3), 0..20),
        ) {
            let substitutions = [
                Substitution::SpecialChars, Substitution::Attributes,
                Substitution::Replacements, Substitution::Macros,
                Substitution::PostReplacements, Substitution::Quotes,
                Substitution::Callouts, Substitution::Normal, Substitution::Verbatim,
            ];
            let operations = operations.into_iter().filter_map(|(index, operation)| {
                substitutions.get(index).map(|substitution| match operation {
                    0 => SubstitutionOp::Append(substitution.clone()),
                    1 => SubstitutionOp::Prepend(substitution.clone()),
                    _ => SubstitutionOp::Remove(substitution.clone()),
                })
            }).collect();
            let spec = SubstitutionSpec::Modifiers(operations);
            for defaults in [NORMAL, VERBATIM, &[]] {
                let resolved = spec.resolve(defaults);
                for substitution in &substitutions {
                    proptest::prop_assert_eq!(
                        spec.contains(substitution, defaults),
                        resolved.contains(substitution),
                    );
                }
            }
        }

        #[test]
        fn membership_matches_resolved_source_order(
            operations in proptest::collection::vec((0_usize..15, 0_u8..4), 0..20),
        ) {
            let names = [
                "specialchars", "attributes", "replacements", "macros",
                "post_replacements", "quotes", "callouts", "normal", "verbatim",
                "a", "q", "c", "none", "unknown", "",
            ];
            let entries = operations.into_iter().filter_map(|(index, operation)| {
                names.get(index).map(|name| match operation {
                    0 => format!(" {name} "),
                    1 => format!(" +{name} "),
                    2 => format!(" {name}+ "),
                    _ => format!(" -{name} "),
                })
            }).collect();
            let spec = SubstitutionSpec::Source(entries);
            for defaults in [NORMAL, VERBATIM, &[]] {
                let resolved = spec.resolve(defaults);
                for substitution in [
                    Substitution::SpecialChars, Substitution::Attributes,
                    Substitution::Replacements, Substitution::Macros,
                    Substitution::PostReplacements, Substitution::Quotes,
                    Substitution::Callouts, Substitution::Normal, Substitution::Verbatim,
                ] {
                    proptest::prop_assert_eq!(
                        spec.contains(&substitution, defaults),
                        resolved.contains(&substitution),
                    );
                }
            }
        }
    }

    fn explicit(spec: &SubstitutionSpec) -> Vec<Substitution> {
        spec.resolve(&[])
    }

    #[allow(clippy::panic)]
    fn source_entries(spec: &SubstitutionSpec) -> &[String] {
        match spec {
            SubstitutionSpec::Source(entries) => entries,
            SubstitutionSpec::Explicit(_) | SubstitutionSpec::Modifiers(_) => {
                panic!("expected source entries")
            }
        }
    }

    #[test]
    fn test_parse_subs_none() {
        let result = parse_subs_attribute("none");
        assert_eq!(result, SubstitutionSpec::Explicit(Vec::new()));
    }

    #[test]
    fn test_parse_subs_empty_string() {
        let result = parse_subs_attribute("");
        assert_eq!(result, SubstitutionSpec::Explicit(Vec::new()));
    }

    #[test]
    fn test_parse_subs_none_with_whitespace() {
        let result = parse_subs_attribute("  none  ");
        assert_eq!(result, SubstitutionSpec::Explicit(Vec::new()));
    }

    #[test]
    fn test_parse_subs_specialchars() {
        let result = parse_subs_attribute("specialchars");
        assert_eq!(explicit(&result), vec![Substitution::SpecialChars]);
    }

    #[test]
    fn test_parse_subs_specialchars_shorthand() {
        let result = parse_subs_attribute("c");
        assert_eq!(explicit(&result), vec![Substitution::SpecialChars]);
    }

    #[test]
    fn test_parse_subs_specialcharacters_alias() {
        let result = parse_subs_attribute("specialcharacters");
        assert_eq!(explicit(&result), vec![Substitution::SpecialChars]);
    }

    #[test]
    fn test_parse_subs_normal_expands() {
        let result = parse_subs_attribute("normal");
        assert_eq!(explicit(&result), NORMAL.to_vec());
    }

    #[test]
    fn test_parse_subs_verbatim_expands() {
        let result = parse_subs_attribute("verbatim");
        assert_eq!(explicit(&result), VERBATIM.to_vec());
    }

    #[test]
    fn test_parse_subs_append_modifier() {
        let result = parse_subs_attribute("+quotes");
        assert_eq!(source_entries(&result), ["+quotes"]);

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
        assert_eq!(source_entries(&result), ["quotes+"]);

        // Verify resolved result with VERBATIM baseline
        let resolved = result.resolve(VERBATIM);
        assert_eq!(resolved.first(), Some(&Substitution::Quotes));
        assert!(resolved.contains(&Substitution::SpecialChars));
        assert!(resolved.contains(&Substitution::Callouts));
    }

    #[test]
    fn prepend_modifiers_move_existing_stages_and_preserve_group_order() {
        for (spec, baseline, expected) in [
            (
                "attributes+",
                NORMAL,
                "attributes,special_chars,quotes,replacements,macros,post_replacements",
            ),
            (
                "+attributes",
                NORMAL,
                "special_chars,quotes,attributes,replacements,macros,post_replacements",
            ),
            (
                "attributes+,quotes+",
                NORMAL,
                "quotes,attributes,special_chars,replacements,macros,post_replacements",
            ),
            (
                "quotes+,attributes+",
                NORMAL,
                "attributes,quotes,special_chars,replacements,macros,post_replacements",
            ),
            (
                "attributes+,attributes+",
                NORMAL,
                "attributes,special_chars,quotes,replacements,macros,post_replacements",
            ),
            (
                "attributes+,normal+",
                NORMAL,
                "special_chars,quotes,attributes,replacements,macros,post_replacements",
            ),
            (
                "normal+,attributes+",
                NORMAL,
                "attributes,special_chars,quotes,replacements,macros,post_replacements",
            ),
            (
                "verbatim+",
                NORMAL,
                "special_chars,callouts,quotes,attributes,replacements,macros,post_replacements",
            ),
            (
                "normal+",
                VERBATIM,
                "special_chars,quotes,attributes,replacements,macros,post_replacements,callouts",
            ),
            (
                "+normal",
                VERBATIM,
                "special_chars,callouts,quotes,attributes,replacements,macros,post_replacements",
            ),
            (
                "normal+,verbatim+,normal+",
                VERBATIM,
                "special_chars,quotes,attributes,replacements,macros,post_replacements,callouts",
            ),
            ("-normal,attributes+", NORMAL, "attributes"),
            (
                "attributes+,-attributes",
                NORMAL,
                "special_chars,quotes,replacements,macros,post_replacements",
            ),
            (
                "-attributes,+attributes",
                NORMAL,
                "special_chars,quotes,replacements,macros,post_replacements,attributes",
            ),
            (
                "normal+,-quotes,quotes+",
                VERBATIM,
                "quotes,special_chars,attributes,replacements,macros,post_replacements,callouts",
            ),
            (
                "verbatim+,-specialchars,+specialchars",
                NORMAL,
                "callouts,quotes,attributes,replacements,macros,post_replacements,special_chars",
            ),
            (
                "+quotes,attributes+",
                &[] as &[Substitution],
                "attributes,quotes",
            ),
        ] {
            let actual = parse_subs_attribute(spec).resolve(baseline);
            assert_eq!(
                actual
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
                expected,
                "{spec} with {baseline:?}"
            );
        }
    }

    #[test]
    fn test_parse_subs_remove_modifier() {
        let result = parse_subs_attribute("-specialchars");
        assert_eq!(source_entries(&result), ["-specialchars"]);

        // Verify resolved result with VERBATIM baseline
        let resolved = result.resolve(VERBATIM);
        assert!(!resolved.contains(&Substitution::SpecialChars));
        assert!(resolved.contains(&Substitution::Callouts));
    }

    #[test]
    fn test_parse_subs_remove_all_verbatim() {
        let result = parse_subs_attribute("-specialchars,-callouts");
        let ops = source_entries(&result);
        assert_eq!(ops.len(), 2);

        // Verify resolved result with VERBATIM baseline
        let resolved = result.resolve(VERBATIM);
        assert_eq!(resolved, []);
    }

    #[test]
    fn test_parse_subs_combined_modifiers() {
        let result = parse_subs_attribute("+quotes,-callouts");
        let ops = source_entries(&result);
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
            vec![
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
            vec![
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
            vec![Substitution::Quotes, Substitution::Attributes]
        );
    }

    #[test]
    fn test_parse_subs_duplicates_ignored() {
        let result = parse_subs_attribute("quotes,quotes,quotes");
        assert_eq!(explicit(&result), vec![Substitution::Quotes]);
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
        assert_eq!(resolved, []);
    }

    #[test]
    fn test_parse_subs_unknown_is_skipped() {
        // Unknown substitution types are logged and skipped
        let result = parse_subs_attribute("unknown");
        assert_eq!(explicit(&result), []);
    }

    #[test]
    fn test_parse_subs_unknown_mixed_with_valid() {
        // Unknown substitution types are skipped, valid ones are kept
        let result = parse_subs_attribute("quotes,typo,attributes");
        assert_eq!(
            explicit(&result),
            vec![Substitution::Quotes, Substitution::Attributes]
        );
    }

    #[test]
    fn test_parse_subs_all_individual_types() {
        // Test each substitution type can be parsed
        assert_eq!(
            explicit(&parse_subs_attribute("attributes")),
            vec![Substitution::Attributes]
        );
        assert_eq!(
            explicit(&parse_subs_attribute("replacements")),
            vec![Substitution::Replacements]
        );
        assert_eq!(
            explicit(&parse_subs_attribute("macros")),
            vec![Substitution::Macros]
        );
        assert_eq!(
            explicit(&parse_subs_attribute("post_replacements")),
            vec![Substitution::PostReplacements]
        );
        assert_eq!(
            explicit(&parse_subs_attribute("quotes")),
            vec![Substitution::Quotes]
        );
        assert_eq!(
            explicit(&parse_subs_attribute("callouts")),
            vec![Substitution::Callouts]
        );
    }

    #[test]
    fn test_parse_subs_shorthand_types() {
        assert_eq!(
            explicit(&parse_subs_attribute("a")),
            vec![Substitution::Attributes]
        );
        assert_eq!(
            explicit(&parse_subs_attribute("r")),
            vec![Substitution::Replacements]
        );
        assert_eq!(
            explicit(&parse_subs_attribute("m")),
            vec![Substitution::Macros]
        );
        assert_eq!(
            explicit(&parse_subs_attribute("p")),
            vec![Substitution::PostReplacements]
        );
        assert_eq!(
            explicit(&parse_subs_attribute("q")),
            vec![Substitution::Quotes]
        );
        assert_eq!(
            explicit(&parse_subs_attribute("c")),
            vec![Substitution::SpecialChars]
        );
    }

    #[test]
    fn test_parse_subs_mixed_modifier_list() {
        let result = parse_subs_attribute("specialchars,+quotes");
        assert_eq!(
            explicit(&result),
            &[Substitution::SpecialChars, Substitution::Quotes]
        );
        assert_eq!(result.resolve(VERBATIM), explicit(&result));
    }

    #[test]
    fn test_parse_subs_modifier_in_middle() {
        let result = parse_subs_attribute("attributes,+quotes,-callouts");
        assert_eq!(
            explicit(&result),
            &[Substitution::Attributes, Substitution::Quotes]
        );
        assert_eq!(result.resolve(VERBATIM), explicit(&result));
    }

    #[test]
    fn test_parse_subs_asciidoctor_example() {
        // From asciidoctor docs: subs="attributes+,+replacements,-callouts"
        let result = parse_subs_attribute("attributes+,+replacements,-callouts");
        let ops = source_entries(&result);
        assert_eq!(ops.len(), 3);

        // Verify resolved result with VERBATIM baseline
        let resolved = result.resolve(VERBATIM);
        assert_eq!(resolved.first(), Some(&Substitution::Attributes)); // prepended
        assert!(resolved.contains(&Substitution::Replacements)); // appended
        assert!(!resolved.contains(&Substitution::Callouts)); // removed
    }

    #[test]
    fn test_parse_subs_modifier_only_at_end() {
        let result = parse_subs_attribute("quotes,-specialchars");
        assert_eq!(explicit(&result), &[Substitution::Quotes]);
        assert_eq!(result.resolve(VERBATIM), explicit(&result));
    }

    #[test]
    fn plain_first_substitution_lists_ignore_all_block_baselines() -> Result<(), serde_json::Error>
    {
        for (spec, expected) in [
            ("quotes,+attributes", "quotes,attributes"),
            ("quotes,attributes+", "attributes,quotes"),
            ("quotes,-quotes", ""),
            ("quotes,-quotes,+quotes", "quotes"),
            ("none,+quotes", "quotes"),
            ("none,-quotes,+attributes", "attributes"),
            ("quotes,+none", "quotes"),
            ("quotes,none", "quotes"),
            (
                "none,normal,-macros",
                "special_chars,quotes,attributes,replacements,post_replacements",
            ),
            (
                "normal,-macros",
                "special_chars,quotes,attributes,replacements,post_replacements",
            ),
            ("verbatim,-callouts,+quotes", "special_chars,quotes"),
            (
                "verbatim,normal+",
                "special_chars,quotes,attributes,replacements,macros,post_replacements,callouts",
            ),
            (
                "normal,verbatim+",
                "special_chars,callouts,quotes,attributes,replacements,macros,post_replacements",
            ),
            ("none,+normal,-normal", ""),
            ("unknown,+quotes", "quotes"),
            ("none,unknown,+quotes", "quotes"),
            (",+quotes", "quotes"),
            (" , +quotes", "quotes"),
            (",,attributes+", "attributes"),
            ("quotes,,+attributes,", "quotes,attributes"),
            ("quotes,+attributes,-quotes,quotes+", "quotes,attributes"),
            ("quotes,attributes+,+quotes", "attributes,quotes"),
            ("attributes,+quotes,-callouts", "attributes,quotes"),
            ("quotes,-normal,+macros", "macros"),
            ("normal,-normal,+attributes", "attributes"),
            (
                "normal,+replacements",
                "special_chars,quotes,attributes,replacements,macros,post_replacements",
            ),
        ] {
            let parsed = parse_subs_attribute(spec);
            assert_eq!(
                source_entries(&parsed),
                spec.split(',').map(str::trim).collect::<Vec<_>>(),
                "{spec}"
            );
            for baseline in [NORMAL, VERBATIM, &[]] {
                let actual = parsed.resolve(baseline);
                assert_eq!(
                    actual
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                    expected,
                    "{spec} with {baseline:?}"
                );
                assert_eq!(
                    serde_json::to_value(&parsed)?,
                    serde_json::to_value(spec.split(',').map(str::trim).collect::<Vec<_>>())?,
                    "{spec}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn modifier_first_substitution_lists_keep_block_baselines() {
        for spec in [
            "+quotes,attributes",
            "+none,quotes,attributes",
            "-unknown,quotes,attributes",
            "+quotes,,attributes",
        ] {
            let parsed = parse_subs_attribute(spec);
            assert_eq!(
                source_entries(&parsed),
                spec.split(',').map(str::trim).collect::<Vec<_>>(),
                "{spec}"
            );
            assert_eq!(parsed.resolve(NORMAL), NORMAL, "{spec}");
            assert_eq!(
                parsed.resolve(VERBATIM),
                [
                    Substitution::SpecialChars,
                    Substitution::Callouts,
                    Substitution::Quotes,
                    Substitution::Attributes
                ],
                "{spec}"
            );
        }
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
        assert!(
            attributes
                .insert("weight".into(), attribute_weight.clone())
                .is_ok()
        );
        assert!(
            attributes
                .insert("mass".into(), attribute_mass.clone())
                .is_ok()
        );

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
    fn test_resolve_typed_default_and_original_text() {
        let mut attributes = DocumentAttributes::default();
        assert_eq!(
            substitute("depth={max-include-depth}", HEADER, &attributes),
            "depth=64"
        );

        assert!(
            attributes
                .set("max-include-depth".into(), "064".into())
                .is_ok()
        );
        assert_eq!(
            substitute("depth={max-include-depth}", HEADER, &attributes),
            "depth=064"
        );

        assert!(
            attributes
                .set("present".into(), AttributeValue::Bool(true))
                .is_ok()
        );
        assert_eq!(
            substitute("before{present}after", HEADER, &attributes),
            "beforeafter"
        );
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
        assert!(attributes.insert("foo".into(), "{bar}".into()).is_ok());
        assert!(
            attributes
                .insert("bar".into(), "should-not-appear".into())
                .is_ok()
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
    // Later operations can restore individual stages or whole groups.
    #[case::modifier_add_macros("+macros", Substitution::Macros, false)]
    #[case::modifier_add_attributes("+attributes", Substitution::Attributes, false)]
    #[case::modifier_add_post_replacements(
        "+post_replacements",
        Substitution::PostReplacements,
        false
    )]
    #[case::modifier_add_quotes("+quotes", Substitution::Quotes, false)]
    #[case::modifier_add_callouts("+callouts", Substitution::Callouts, false)]
    #[case::remove_normal_disables_macros("-normal", Substitution::Macros, true)]
    #[case::remove_normal_disables_quotes("-normal", Substitution::Quotes, true)]
    #[case::remove_normal_disables_attributes("-normal", Substitution::Attributes, true)]
    #[case::remove_normal_disables_hardbreaks("-normal", Substitution::PostReplacements, true)]
    #[case::remove_normal_keeps_callouts("-normal", Substitution::Callouts, false)]
    #[case::remove_verbatim_disables_callouts("-verbatim", Substitution::Callouts, true)]
    #[case::remove_verbatim_disables_specialchars("-verbatim", Substitution::SpecialChars, true)]
    #[case::remove_verbatim_keeps_quotes("-verbatim", Substitution::Quotes, false)]
    #[case::restore_normal_macros("-normal,+normal", Substitution::Macros, false)]
    #[case::restore_normal_attributes("-normal,+normal", Substitution::Attributes, false)]
    #[case::restore_verbatim_callouts("-verbatim,+verbatim", Substitution::Callouts, false)]
    #[case::restore_only_callouts("-verbatim,+callouts", Substitution::SpecialChars, true)]
    #[case::restore_only_specialchars("-verbatim,+specialchars", Substitution::Callouts, true)]
    #[case::remove_restored_group("-normal,+normal,-normal", Substitution::Quotes, true)]
    #[case::restore_only_macros("-normal,+macros", Substitution::Attributes, true)]
    #[case::restore_only_attributes("-normal,attributes+", Substitution::Macros, true)]
    // `none` is the explicit empty list and disables everything.
    #[case::none_disables_macros("none", Substitution::Macros, true)]
    #[case::none_disables_attributes("none", Substitution::Attributes, true)]
    #[case::none_disables_post_replacements("none", Substitution::PostReplacements, true)]
    #[case::none_disables_quotes("none", Substitution::Quotes, true)]
    #[case::none_disables_callouts("none", Substitution::Callouts, true)]
    fn block_plan_disables_requested_stages(
        #[case] subs_attr: &str,
        #[case] sub: Substitution,
        #[case] expected: bool,
    ) {
        let spec = parse_subs_attribute(subs_attr);
        assert_eq!(
            !SubstitutionPlan::for_block_spec(&spec).enabled(&sub),
            expected,
            "spec={subs_attr:?} sub={sub:?}"
        );
    }

    #[test]
    fn removed_group_plans_keep_restored_stages_in_order() {
        for (spec, expected) in [
            ("-normal", "callouts"),
            ("-normal,attributes+", "attributes,callouts"),
            ("-normal,+attributes,+macros", "callouts,attributes,macros"),
            ("-normal,+macros,+attributes", "callouts,macros,attributes"),
            (
                "-normal,+normal",
                "callouts,specialchars,quotes,attributes,replacements,macros,post_replacements",
            ),
            (
                "-normal,normal+,attributes+",
                "attributes,specialchars,quotes,replacements,macros,post_replacements,callouts",
            ),
            (
                "-verbatim",
                "quotes,attributes,replacements,macros,post_replacements",
            ),
            (
                "-verbatim,+verbatim",
                "quotes,attributes,replacements,macros,post_replacements,specialchars,callouts",
            ),
            (
                "-verbatim,+callouts",
                "quotes,attributes,replacements,macros,post_replacements,callouts",
            ),
            ("-normal,-verbatim", "none"),
            ("-normal,+quotes,-normal", "callouts"),
        ] {
            assert_eq!(
                SubstitutionPlan::for_block_spec(&parse_subs_attribute(spec)),
                SubstitutionPlan::from_substitutions(&parse_subs_attribute(expected).resolve(&[])),
                "{spec}"
            );
        }
    }
}

/// Expand known attribute references once, leaving unresolved references unchanged.
///
/// Names contain one or more ASCII letters, digits, underscores, or hyphens.
/// A backslash before a valid reference is removed and protects that reference
/// from lookup. Only valid, unescaped references call `lookup`; inserted values
/// are not scanned again. Other text and line endings are preserved.
///
/// The result borrows the input when no reference is replaced or escape removed.
#[must_use]
pub fn substitute_attributes<'text, 'value>(
    text: &'text str,
    lookup: impl FnMut(&str) -> Option<&'value super::DocumentAttributeValue<'value>>,
) -> Cow<'text, str> {
    substitute_attributes_with_ranges(text, lookup, |_, _| {})
}

/// Resolve attribute source and return a diagnostic for unsupported profiles.
pub(crate) fn resolve_attribute_entry_text<'text>(
    text: &'text str,
    attributes: &DocumentAttributes<'_>,
) -> (super::DocumentAttributeValue<'text>, Option<String>) {
    if let Some(profile) = attribute_inline_profile(text, attributes) {
        return match profile {
            Ok((inner, mut substitutions)) => {
                let inner = if substitutions.contains(&Substitution::Attributes) {
                    expand_attribute_entry_references(inner, attributes, false).into()
                } else {
                    Cow::Borrowed(inner)
                };
                // References are frozen at assignment. Structural rules run at
                // the use site and must not expand those references again.
                substitutions.retain(|stage| *stage != Substitution::Attributes);
                let fragment = super::AttributeInlineFragment {
                    range: 0..inner.len(),
                    substitutions: substitutions.into_boxed_slice(),
                };
                (
                    super::DocumentAttributeValue::with_inline_fragments(
                        inner,
                        Vec::new(),
                        vec![fragment],
                    ),
                    None,
                )
            }
            Err(message) => (
                super::DocumentAttributeValue::with_inline_fragments(
                    Cow::Borrowed(text),
                    Vec::new(),
                    vec![super::AttributeInlineFragment {
                        range: 0..text.len(),
                        substitutions: vec![Substitution::SpecialChars].into_boxed_slice(),
                    }],
                ),
                Some(message),
            ),
        };
    }
    if let Some((inner, substitutions)) = attribute_text_substitutions(text) {
        if substitutions == [AttributeTextSubstitution::Attributes] {
            let value = resolve_attribute_references(inner, attributes, true);
            if !value.inline_fragments().is_empty() {
                return (value, None);
            }
        }
        let mut inner = Cow::Borrowed(inner);
        for substitution in substitutions {
            inner = match substitution {
                AttributeTextSubstitution::Attributes => {
                    expand_attribute_entry_references(&inner, attributes, true).into()
                }
                AttributeTextSubstitution::SpecialChars => {
                    escape_attribute_characters(&inner).into()
                }
            };
        }
        let ranges = if inner.is_empty() {
            Vec::new()
        } else {
            std::iter::once(0..inner.len()).collect()
        };
        return (
            super::DocumentAttributeValue::with_passthrough_ranges(inner, ranges),
            None,
        );
    }
    (resolve_attribute_references(text, attributes, false), None)
}

fn resolve_attribute_references<'text>(
    text: &'text str,
    attributes: &DocumentAttributes<'_>,
    protect_gaps: bool,
) -> super::DocumentAttributeValue<'text> {
    let mut ranges = Vec::new();
    let mut fragments = Vec::new();
    let text = substitute_attributes_with_ranges(
        text,
        |name| attributes.get(name),
        |value, offset| {
            fragments.extend(
                value
                    .inline_fragments()
                    .iter()
                    .cloned()
                    .map(|mut fragment| {
                        fragment.range = fragment.range.start + offset..fragment.range.end + offset;
                        fragment
                    }),
            );
            ranges.extend(
                value
                    .passthrough_ranges()
                    .iter()
                    .map(|range| range.start + offset..range.end + offset),
            );
        },
    );
    if protect_gaps && !fragments.is_empty() {
        // An explicit `a` list imports referenced profiles but enables no
        // structural rules for the surrounding source.
        let mut gaps = Vec::new();
        let mut cursor = 0;
        for fragment in &fragments {
            if cursor < fragment.range.start {
                gaps.push(cursor..fragment.range.start);
            }
            cursor = fragment.range.end;
        }
        if cursor < text.len() {
            gaps.push(cursor..text.len());
        }
        fragments.extend(
            gaps.into_iter()
                .map(|range| super::AttributeInlineFragment {
                    range,
                    substitutions: Box::default(),
                }),
        );
    }
    super::DocumentAttributeValue::with_inline_fragments(text, ranges, fragments)
}

/// Select retained source rules, rejecting lists that require converted markup.
fn attribute_inline_profile<'text>(
    text: &'text str,
    attributes: &DocumentAttributes<'_>,
) -> Option<Result<(&'text str, Vec<Substitution>), String>> {
    let (names, inner) = text.trim().strip_prefix("pass:")?.split_once('[')?;
    let inner = inner.strip_suffix(']')?;
    let mut substitutions = Vec::new();
    for name in names
        .split(',')
        .filter(|name| !name.is_empty() && *name != "none")
    {
        let Some(stage) = parse_substitution(name) else {
            return Some(Err(format!(
                "unknown attribute-value substitution: {name}; value retained literally"
            )));
        };
        substitutions.push(stage);
    }
    let substitutions = resolve_passthrough_substitutions(&substitutions);
    if !substitutions.iter().any(|stage| {
        matches!(
            stage,
            Substitution::Quotes
                | Substitution::Macros
                | Substitution::Replacements
                | Substitution::PostReplacements
        )
    }) {
        if substitutions.contains(&Substitution::Attributes)
            && substitutions.contains(&Substitution::SpecialChars)
            && !resolve_attribute_references(inner, attributes, false)
                .inline_fragments()
                .is_empty()
        {
            return Some(Err("attribute-value escaping combined with formatted references is unsupported; value retained literally".into()));
        }
        return None;
    }
    let mut structural = false;
    for stage in &substitutions {
        if *stage == Substitution::SpecialChars && structural {
            return Some(Err("attribute-value escaping after formatting or macros requires backend markup; value retained literally".into()));
        }
        structural |= matches!(stage, Substitution::Quotes | Substitution::Macros);
    }
    Some(Ok((inner, substitutions)))
}

#[derive(PartialEq, Eq)]
pub(crate) enum AttributeTextSubstitution {
    Attributes,
    SpecialChars,
}

fn attribute_text_substitutions(text: &str) -> Option<(&str, Vec<AttributeTextSubstitution>)> {
    let (names, inner) = text.trim().strip_prefix("pass:")?.split_once('[')?;
    let inner = inner.strip_suffix(']')?;
    Some((inner, attribute_text_substitution_names(names)?))
}

pub(crate) fn attribute_text_substitution_names(
    names: &str,
) -> Option<Vec<AttributeTextSubstitution>> {
    let mut substitutions = Vec::new();
    if !names.is_empty() {
        for name in names.split(',') {
            let substitution = match name {
                "none" => continue,
                "a" | "attributes" => AttributeTextSubstitution::Attributes,
                "c" | "specialchars" | "specialcharacters" | "v" | "verbatim" => {
                    AttributeTextSubstitution::SpecialChars
                }
                _ => return None,
            };
            if !substitutions.contains(&substitution) {
                substitutions.push(substitution);
            }
        }
    }
    Some(substitutions)
}

pub(crate) fn escape_attribute_characters(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Find a reference-shaped token, including its existing `amp;` escaping.
pub(crate) fn character_reference_end(text: &str) -> Option<usize> {
    let tail = text.strip_prefix('&')?;
    let first = tail.find(';')?;
    if tail[..first].is_empty()
        || !tail[..first]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'#')
    {
        return None;
    }
    let mut unescaped = tail;
    while let Some(rest) = unescaped.strip_prefix("amp;") {
        unescaped = rest;
    }
    let end = unescaped.find(';').filter(|end| {
        !unescaped[..*end].is_empty()
            && unescaped[..*end]
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'#')
    });
    Some(
        end.map_or((text.len() - unescaped.len()).max(first + 2), |end| {
            text.len() - unescaped.len() + end + 1
        }),
    )
}

pub(crate) fn is_restorable_character_reference(body: &str) -> bool {
    // Asciidoctor restores only this subset during replacements. In particular,
    // a one-digit numeric reference and an uppercase hex prefix stay escaped.
    if let Some(digits) = body.strip_prefix("#x") {
        (2..=5).contains(&digits.len()) && digits.bytes().all(|byte| byte.is_ascii_hexdigit())
    } else if let Some(digits) = body.strip_prefix('#') {
        (2..=6).contains(&digits.len()) && digits.bytes().all(|byte| byte.is_ascii_digit())
    } else {
        let letters = body.bytes().take_while(u8::is_ascii_alphabetic).count();
        letters >= 2
            && body.len() - letters <= 2
            && body[letters..].bytes().all(|byte| byte.is_ascii_digit())
    }
}

fn expand_attribute_entry_references(
    text: &str,
    attributes: &DocumentAttributes<'_>,
    escape_document_values: bool,
) -> String {
    let mut output = String::with_capacity(text.len());
    let mut remaining = text;
    while let Some((prefix, candidate)) = remaining.split_once('{') {
        let Some((name, rest)) = candidate.split_once('}') else {
            break;
        };
        if prefix.ends_with('\\') || name.ends_with('\\') {
            output.push_str(prefix.strip_suffix('\\').unwrap_or(prefix));
            output.push('{');
            output.push_str(name.strip_suffix('\\').unwrap_or(name));
            output.push('}');
        } else if let Some(value) = attributes.get(name) {
            output.push_str(prefix);
            let mut value_text = String::new();
            let _ = value.write_text(&mut value_text);
            if escape_document_values && attributes.is_document_value(name) {
                // Ordinary entries already underwent header escaping. Raw ranges
                // contain prepared text and must not be escaped a second time.
                let mut cursor = 0;
                for range in value.passthrough_ranges() {
                    output.push_str(&escape_attribute_characters(
                        &value_text[cursor..range.start],
                    ));
                    output.push_str(&value_text[range.clone()]);
                    cursor = range.end;
                }
                output.push_str(&escape_attribute_characters(&value_text[cursor..]));
            } else {
                output.push_str(&value_text);
            }
        } else {
            output.push_str(prefix);
            output.push('{');
            output.push_str(name);
            output.push('}');
        }
        remaining = rest;
    }
    output.push_str(remaining);
    output
}

fn substitute_attributes_with_ranges<'text, 'value>(
    text: &'text str,
    mut lookup: impl FnMut(&str) -> Option<&'value super::DocumentAttributeValue<'value>>,
    mut substituted: impl FnMut(&super::DocumentAttributeValue<'value>, usize),
) -> Cow<'text, str> {
    let mut remaining = text;
    let mut unwritten = text;
    let mut output: Option<String> = None;
    while let Some((prefix, candidate)) = remaining.split_once('{') {
        let name_len = candidate
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'-'))
            .count();
        let (name, after_name) = candidate.split_at(name_len);
        let Some(rest) = after_name.strip_prefix('}') else {
            remaining = after_name;
            continue;
        };
        remaining = rest;
        if name.is_empty() {
            continue;
        }
        let prefix_len = unwritten.len() - candidate.len() - 1;
        if prefix.ends_with('\\') {
            let output = output.get_or_insert_with(|| String::with_capacity(text.len()));
            output.push_str(unwritten.split_at(prefix_len - 1).0);
            output.push('{');
            output.push_str(name);
            output.push('}');
            unwritten = rest;
        } else if let Some(value) = lookup(name) {
            let output = output.get_or_insert_with(|| String::with_capacity(text.len()));
            output.push_str(unwritten.split_at(prefix_len).0);
            substituted(value, output.len());
            let _ = value.write_text(output);
            unwritten = rest;
        }
    }
    match output {
        Some(mut output) => {
            output.push_str(unwritten);
            Cow::Owned(output)
        }
        None => Cow::Borrowed(text),
    }
}

#[cfg(test)]
mod attribute_substitution_tests {
    use std::borrow::Cow;

    use crate::{DocumentAttributeValue, Options};

    use super::substitute_attributes;

    #[rstest::rstest]
    #[case(r"\{shell}", "{shell}")]
    #[case(r"\\{shell}", r"\{shell}")]
    #[case(r"\\\{missing}", r"\\{missing}")]
    fn escaped_references_do_not_call_lookup(#[case] input: &str, #[case] expected: &str) {
        let value = DocumentAttributeValue::from("unexpected");
        let mut names = Vec::new();
        let output = substitute_attributes(input, |name| {
            names.push(name.to_owned());
            Some(&value)
        });
        assert_eq!(output, expected);
        assert_eq!(names, [] as [String; 0]);
    }

    #[rstest::rstest]
    #[case("λ plain\r\ntext\n", &[])]
    #[case("{missing}", &["missing"])]
    #[case("{{missing}}", &["missing"])]
    #[case("{} {bad name} {name.part} {café} {counter:n}", &[])]
    #[case(r"\{bad name} {shell\} ${shell:-fallback}", &[])]
    #[case("{unfinished", &[])]
    fn unchanged_text_stays_borrowed(#[case] input: &str, #[case] expected_names: &[&str]) {
        let mut names = Vec::new();
        let output = substitute_attributes(input, |name| {
            names.push(name.to_owned());
            None
        });
        assert!(matches!(output, Cow::Borrowed(value) if std::ptr::eq(value, input)));
        assert_eq!(names, expected_names);
    }

    #[test]
    fn adjacent_escaped_known_and_unknown_references_are_distinct() {
        let value = DocumentAttributeValue::from("zsh");
        let mut names = Vec::new();
        let output = substitute_attributes(r"{shell}\{shell}{missing}{shell}", |name| {
            names.push(name.to_owned());
            (name == "shell").then_some(&value)
        });
        assert_eq!(output, "zsh{shell}{missing}zsh");
        assert_eq!(names, ["shell", "missing", "shell"]);
    }

    #[rstest::rstest]
    #[case("{{shell}}", "{zsh}", 1)]
    #[case("{outer{shell}}", "{outerzsh}", 1)]
    #[case(r"\{{shell}}", r"\{zsh}", 1)]
    #[case(r"{\{shell}}", "{{shell}}", 0)]
    #[case(r"\{outer{shell}}", r"\{outerzsh}", 1)]
    #[case("{é{shell}}", "{ézsh}", 1)]
    fn valid_inner_references_expand_once(
        #[case] input: &str,
        #[case] expected: &str,
        #[case] calls: usize,
    ) {
        let value = DocumentAttributeValue::from("zsh");
        let mut names = Vec::new();
        let output = substitute_attributes(input, |name| {
            names.push(name.to_owned());
            Some(&value)
        });
        assert_eq!(output, expected);
        assert_eq!(names, vec!["shell"; calls]);
    }

    #[test]
    fn names_follow_the_parser_reference_grammar() {
        let value = DocumentAttributeValue::from("x");
        let mut names = Vec::new();
        let output = substitute_attributes("{_}{-}{0}{A-b_2}", |name| {
            names.push(name.to_owned());
            Some(&value)
        });
        assert_eq!(output, "xxxx");
        assert_eq!(names, ["_", "-", "0", "A-b_2"]);
    }

    #[test]
    fn replacement_text_is_not_scanned_again() {
        let first = DocumentAttributeValue::from("{second}\\{third}\r\n");
        let second = DocumentAttributeValue::from("done");
        let mut names = Vec::new();
        let output = substitute_attributes("{first}/{second}", |name| {
            names.push(name.to_owned());
            match name {
                "first" => Some(&first),
                "second" => Some(&second),
                _ => None,
            }
        });
        assert_eq!(output, "{second}\\{third}\r\n/done");
        assert_eq!(names, ["first", "second"]);
    }

    #[test]
    fn values_keep_lexical_spelling_and_empty_values_are_present() -> Result<(), crate::Error> {
        let options = Options::builder()
            .with_attribute("max-include-depth", "003")
            .with_attribute("quoted", "\"word\"")
            .with_attribute("empty", "")
            .with_attribute("present", true)
            .build()?;
        let attributes = options.document_attributes();
        let output = substitute_attributes(
            "\r\n{max-include-depth}/{quoted}/{empty}/{present}\n",
            |name| attributes.get(name),
        );
        assert_eq!(output, "\r\n003/\"word\"//\n");
        Ok(())
    }
}
