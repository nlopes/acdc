//! Text substitution utilities for `AsciiDoc` converters.
//!
//! This module provides functions for processing `AsciiDoc` text substitutions
//! that are common across different output formats (HTML, terminal, etc.).

use std::borrow::Cow;

use acdc_parser::{InlineMacro, InlineNode, NORMAL, Substitution, VERBATIM};

use crate::TraversalContext;

#[cfg(feature = "pre-spec-subs")]
use acdc_parser::SubstitutionSpec;

#[cfg(feature = "pre-spec-subs")]
use bitflags::bitflags;

/// Expand known attribute references, leaving unresolved references unchanged.
///
/// The result borrows `text` when no reference is replaced or escape removed.
#[must_use]
pub fn substitute_attributes<'text>(
    text: &'text str,
    attributes: &TraversalContext<'_>,
) -> Cow<'text, str> {
    attributes.substitute_attributes(text)
}

#[cfg(feature = "pre-spec-subs")]
bitflags! {
    /// Compact representation of the substitutions active for the block a
    /// converter is currently rendering.
    ///
    /// Returned by [`effective_subs_flags`] and stored on the per-converter
    /// `Processor` so that the inline-rendering hot path can ask
    /// "is `X` substitution enabled?" in O(1) without traversing a
    /// `Vec<Substitution>` or paying for runtime borrow tracking.
    ///
    /// This is the converter-side counterpart to the parser's internal
    /// parse-time flags; both types are tiny `u8` bitsets that happen to
    /// share variant names. The parser only ever needs the parse-time
    /// subset, so its version stays crate-private.
    ///
    /// Only available when the `pre-spec-subs` feature is enabled — when
    /// it's off, `[subs="…"]` block attributes are silently ignored at the
    /// parser layer, so this bitset would always be `all()` and is omitted.
    ///
    /// The grouping variants `Normal` and `Verbatim` expand during
    /// resolution and never appear in the bitset.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SubsFlags: u8 {
        /// Macros are recognised in the inline grammar.
        const MACROS = 1 << 0;
        /// `{attr}` attribute references are expanded.
        const ATTRIBUTES = 1 << 1;
        /// Trailing-`+` hard line breaks produce `LineBreak` nodes.
        const POST_REPLACEMENTS = 1 << 2;
        /// Inline formatting (`*bold*`, `_italic_`, etc.) is recognised.
        const QUOTES = 1 << 3;
        /// `<1>` / `<.>` callout markers are recognised inside verbatim content.
        const CALLOUTS = 1 << 4;
        /// HTML-style escaping of `<`, `>`, `&` to entities. Only the HTML
        /// converter acts on this; other backends ignore it.
        const SPECIAL_CHARS = 1 << 5;
        /// Typography substitutions (em-dashes, arrows, ellipses, ©, ™, ®).
        /// HTML emits entities; terminal/manpage emit Unicode characters.
        const REPLACEMENTS = 1 << 6;
    }
}

#[cfg(feature = "pre-spec-subs")]
impl Default for SubsFlags {
    /// All substitutions enabled — matches the asciidoctor default when no
    /// `[subs="…"]` attribute is present.
    fn default() -> Self {
        Self::all()
    }
}

#[cfg(feature = "pre-spec-subs")]
impl SubsFlags {
    /// Mapping between flag bits and their corresponding
    /// [`Substitution`] variants. Used by [`Self::from_resolved`] and by
    /// callers that need to round-trip between the two representations.
    pub const FLAG_SUBSTITUTIONS: &'static [(SubsFlags, Substitution)] = &[
        (SubsFlags::MACROS, Substitution::Macros),
        (SubsFlags::ATTRIBUTES, Substitution::Attributes),
        (SubsFlags::POST_REPLACEMENTS, Substitution::PostReplacements),
        (SubsFlags::QUOTES, Substitution::Quotes),
        (SubsFlags::CALLOUTS, Substitution::Callouts),
        (SubsFlags::SPECIAL_CHARS, Substitution::SpecialChars),
        (SubsFlags::REPLACEMENTS, Substitution::Replacements),
    ];

    /// Project a resolved substitution list (as returned by
    /// [`SubstitutionSpec::resolve`]) into a [`SubsFlags`] bitset.
    #[must_use]
    pub(crate) fn from_resolved(subs: &[Substitution]) -> Self {
        let mut flags = Self::empty();
        for sub in subs {
            for (flag, variant) in Self::FLAG_SUBSTITUTIONS {
                if sub == variant {
                    flags |= *flag;
                    break;
                }
            }
        }
        flags
    }
}

/// Resolve a [`SubstitutionSpec`] against the baseline for the block kind.
///
/// Verbatim blocks (listing, literal) use [`VERBATIM`] as the baseline;
/// every other block uses [`NORMAL`]. Returns the concrete list of
/// substitutions a converter should apply to the block's text.
///
/// Converters call this once per block (typically right before walking the
/// inlines) and then query the returned list with `subs.contains(...)`.
///
/// # Behaviour
///
/// - `spec = None` — returns the baseline as-is (block has no `[subs="…"]`).
/// - `spec = Some(Source(...))` — resolves the authored list against the baseline.
/// - `spec = Some(Explicit(...))` — returns the explicit list verbatim.
/// - `spec = Some(Modifiers(...))` — applies the `+`/`-` modifiers to the
///   baseline.
///
/// Only available when the `pre-spec-subs` feature is enabled. When the
/// feature is off, callers should use the baseline directly via
/// [`baseline_subs`].
#[cfg(feature = "pre-spec-subs")]
#[must_use]
pub fn effective_subs(spec: Option<&SubstitutionSpec>, is_verbatim: bool) -> Vec<Substitution> {
    let baseline = if is_verbatim { VERBATIM } else { NORMAL };
    match spec {
        Some(s) => s.resolve(baseline),
        None => baseline.to_vec(),
    }
}

/// Return the block-kind baseline substitution list, ignoring any `[subs="…"]`
/// override. Always available — when `pre-spec-subs` is on, callers usually
/// prefer [`effective_subs`] which folds the override in.
#[must_use]
pub fn baseline_subs(is_verbatim: bool) -> Vec<Substitution> {
    let baseline = if is_verbatim { VERBATIM } else { NORMAL };
    baseline.to_vec()
}

/// Like [`effective_subs`] but returns a compact [`SubsFlags`] bitset.
///
/// Preferred over [`effective_subs`] when the converter only ever queries
/// membership: the result is a single `u8`, contains-checks become a single
/// AND, and there's no per-block heap allocation.
///
/// Only available when the `pre-spec-subs` feature is enabled.
#[cfg(feature = "pre-spec-subs")]
#[must_use]
pub fn effective_subs_flags(spec: Option<&SubstitutionSpec>, is_verbatim: bool) -> SubsFlags {
    SubsFlags::from_resolved(&effective_subs(spec, is_verbatim))
}

/// Paragraph boundaries and neighboring text represented by one fragment.
///
/// A fragment can touch only one end of its paragraph when inline nodes split
/// the source. Em-dash replacement must not treat an internal node boundary as
/// whitespace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextBoundaries {
    at_paragraph_start: bool,
    at_paragraph_end: bool,
    previous: TextNeighbors,
    next: TextNeighbors,
    ordinary_replacements: bool,
}

impl TextBoundaries {
    /// Both fragment ends are paragraph boundaries.
    pub const BOTH: Self = Self::new(true, true);
    /// Neither fragment end is a paragraph boundary.
    pub const NONE: Self = Self::new(false, false);

    /// Record whether each fragment end is a paragraph boundary.
    #[must_use]
    pub const fn new(at_paragraph_start: bool, at_paragraph_end: bool) -> Self {
        Self {
            at_paragraph_start,
            at_paragraph_end,
            previous: TextNeighbors::new(at_paragraph_start),
            next: TextNeighbors::new(at_paragraph_end),
            ordinary_replacements: true,
        }
    }

    /// Supply neighboring characters for word-bounded replacements.
    ///
    /// These characters are context only; they are not replaced or consumed.
    #[must_use]
    pub const fn with_neighbors(mut self, previous: Option<char>, next: Option<char>) -> Self {
        self.previous = TextNeighbors::new(self.at_paragraph_start && previous.is_none());
        self.next = TextNeighbors::new(self.at_paragraph_end && next.is_none());
        self.previous.characters[0] = previous;
        self.next.characters[0] = next;
        self
    }

    /// Permit replacements to consume adjacent ordinary text when enabled.
    ///
    /// Passthroughs retain their own substitution settings. Disabled text can
    /// supply word context, but its spaces and punctuation cannot be consumed.
    #[must_use]
    pub const fn with_ordinary_replacements(mut self, enabled: bool) -> Self {
        self.ordinary_replacements = enabled;
        self
    }

    /// Supply the remaining source when measuring a rendered prefix.
    ///
    /// Unlike [`Self::with_neighbors`], this text belongs to the same
    /// replacement run. A match can consume its characters, but emits text
    /// only when its first dash belongs to the measured prefix.
    #[must_use]
    pub fn with_following_text(mut self, text: &str) -> Self {
        let mut next = TextNeighbors::new(self.at_paragraph_end);
        for character in text.chars() {
            if !next.push(character, self.ordinary_replacements) {
                next.paragraph_boundary = false;
                break;
            }
        }
        self.at_paragraph_end = next.touches_boundary();
        self.next = next;
        self
    }

    /// Retain word context around one inline fragment.
    ///
    /// Visible index terms contribute their label's edges; hidden terms and
    /// ordinary anchors contribute no text. Formatting and other macros remain
    /// barriers. Consumed characters and generated text retain their source owners.
    #[must_use]
    pub fn for_inline(self, nodes: &[InlineNode<'_>], index: usize) -> Self {
        // Invisible nodes need no typography context. Skipping their scans
        // keeps consecutive hidden terms from causing quadratic work.
        if matches!(nodes.get(index), Some(InlineNode::InlineAnchor(anchor)) if !anchor.is_bibliography())
            || matches!(nodes.get(index), Some(InlineNode::Macro(InlineMacro::IndexTerm(term))) if !term.is_visible())
        {
            return Self::NONE.with_ordinary_replacements(self.ordinary_replacements);
        }
        let mut before = TextNeighbors::from_nodes(
            nodes.iter().take(index).rev(),
            false,
            self.previous,
            self.ordinary_replacements,
        );
        let mut after = TextNeighbors::from_nodes(
            nodes.iter().skip(index.saturating_add(1)),
            true,
            self.next,
            self.ordinary_replacements,
        );
        // Raw fragments own a separate substitution profile. They may borrow
        // word context, but cannot consume another profile's source text.
        if matches!(nodes.get(index), Some(InlineNode::RawText(_))) {
            before.replaceable = 0;
            after.replaceable = 0;
        }
        let continuous = matches!(
            nodes.get(index),
            Some(
                InlineNode::PlainText(_)
                    | InlineNode::VerbatimText(_)
                    | InlineNode::RawText(_)
                    | InlineNode::Macro(InlineMacro::IndexTerm(_))
            )
        );
        let at_paragraph_start = before.touches_boundary();
        let at_paragraph_end = after.touches_boundary();
        Self {
            at_paragraph_start,
            at_paragraph_end,
            previous: if continuous {
                before
            } else {
                TextNeighbors::new(at_paragraph_start)
            },
            next: if continuous {
                after
            } else {
                TextNeighbors::new(at_paragraph_end)
            },
            ordinary_replacements: self.ordinary_replacements,
        }
    }

    fn has_adjacent_trigger(self) -> bool {
        self.previous
            .characters
            .iter()
            .chain(&self.next.characters)
            .any(|ch| matches!(ch, Some('-' | '\'')))
    }

    /// Whether the fragment starts at a paragraph boundary.
    #[must_use]
    pub const fn at_paragraph_start(self) -> bool {
        self.at_paragraph_start
    }

    /// Whether the fragment ends at a paragraph boundary.
    #[must_use]
    pub const fn at_paragraph_end(self) -> bool {
        self.at_paragraph_end
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TextNeighbors {
    // Four characters cover a spaced dash or an escaped pair across fragments.
    characters: [Option<char>; 4],
    replaceable: u8,
    prefix_consumed: usize,
    paragraph_boundary: bool,
}

impl TextNeighbors {
    const fn new(paragraph_boundary: bool) -> Self {
        Self {
            characters: [None; 4],
            replaceable: 0,
            prefix_consumed: 0,
            paragraph_boundary,
        }
    }

    fn touches_boundary(self) -> bool {
        self.characters[0].is_none() && self.paragraph_boundary || self.characters[0] == Some('\n')
    }

    fn push(&mut self, character: char, replaceable: bool) -> bool {
        let Some(index) = self.characters.iter().position(Option::is_none) else {
            return false;
        };
        if let Some(slot) = self.characters.get_mut(index) {
            *slot = Some(character);
        }
        if replaceable {
            self.replaceable |= 1 << index;
        }
        true
    }

    fn from_nodes<'n, 's: 'n>(
        nodes: impl Iterator<Item = &'n InlineNode<'s>>,
        start: bool,
        outer: Self,
        ordinary_replacements: bool,
    ) -> Self {
        let mut context = Self::new(false);
        let mut preceding = Vec::new();
        let stop = context.collect(nodes, start, ordinary_replacements, &mut preceding);
        let mut inherited_skip = 0;
        let boundary = match stop {
            NeighborStop::Exhausted => {
                for (index, character) in outer.characters.iter().enumerate() {
                    if let Some(character) = character
                        && !context.collect_character(
                            *character,
                            outer.replaceable & (1 << index) != 0,
                            start,
                            &mut preceding,
                        )
                    {
                        return context.finish(preceding, false, 0);
                    }
                }
                inherited_skip = outer.prefix_consumed;
                outer.paragraph_boundary
            }
            NeighborStop::LineBreak => true,
            NeighborStop::Barrier | NeighborStop::Full => false,
        };
        context.finish(preceding, boundary, inherited_skip)
    }

    fn collect_character(
        &mut self,
        character: char,
        replaceable: bool,
        start: bool,
        preceding: &mut Vec<(char, bool)>,
    ) -> bool {
        if self.push(character, replaceable) {
            return true;
        }
        if start {
            return false;
        }
        if !matches!(self.characters[3], Some('-' | ' ' | '\n')) {
            return false;
        }
        // Repeated spaced pairs share a delimiter. Read the preceding dash
        // run to determine whether its first visible space was consumed.
        let repeated_space = matches!(character, ' ' | '\n')
            && preceding
                .last()
                .is_some_and(|&(previous, _)| matches!(previous, ' ' | '\n'));
        preceding.push((character, replaceable));
        replaceable && !repeated_space && matches!(character, '-' | ' ' | '\n')
    }

    fn finish(
        mut self,
        preceding: Vec<(char, bool)>,
        boundary: bool,
        inherited_skip: usize,
    ) -> Self {
        self.paragraph_boundary = boundary && preceding.is_empty();
        if preceding.is_empty() {
            self.prefix_consumed = inherited_skip;
            return self;
        }
        let mut characters = preceding.into_iter().rev().collect::<Vec<_>>();
        let prefix = characters.len();
        characters.extend(
            self.characters
                .iter()
                .enumerate()
                .rev()
                .filter_map(|(index, ch)| ch.map(|ch| (ch, self.replaceable & (1 << index) != 0))),
        );
        self.prefix_consumed = consumed_dash_prefix(&characters, prefix, boundary, inherited_skip);
        self
    }

    fn collect<'n, 's: 'n>(
        &mut self,
        nodes: impl Iterator<Item = &'n InlineNode<'s>>,
        start: bool,
        ordinary_replacements: bool,
        preceding: &mut Vec<(char, bool)>,
    ) -> NeighborStop {
        for node in nodes {
            let (text, replaceable) = match node {
                InlineNode::PlainText(text) => {
                    (text.content, ordinary_replacements && !text.escaped)
                }
                InlineNode::VerbatimText(text) => (text.content, ordinary_replacements),
                InlineNode::RawText(text) => (text.content, false),
                InlineNode::LineBreak(_) => return NeighborStop::LineBreak,
                InlineNode::InlineAnchor(anchor) if !anchor.is_bibliography() => continue,
                InlineNode::Macro(InlineMacro::IndexTerm(term)) => {
                    if term.is_visible() {
                        let stop = if start {
                            self.collect(term.term().iter(), true, ordinary_replacements, preceding)
                        } else {
                            self.collect(
                                term.term().iter().rev(),
                                false,
                                ordinary_replacements,
                                preceding,
                            )
                        };
                        if stop != NeighborStop::Exhausted {
                            return stop;
                        }
                    }
                    continue;
                }
                InlineNode::BoldText(_)
                | InlineNode::ItalicText(_)
                | InlineNode::MonospaceText(_)
                | InlineNode::HighlightText(_)
                | InlineNode::SubscriptText(_)
                | InlineNode::SuperscriptText(_)
                | InlineNode::CurvedQuotationText(_)
                | InlineNode::CurvedApostropheText(_)
                | InlineNode::StandaloneCurvedApostrophe(_)
                | InlineNode::InlineAnchor(_)
                | InlineNode::Macro(_)
                | InlineNode::CalloutRef(_)
                | _ => return NeighborStop::Barrier,
            };
            if start {
                for character in text.chars() {
                    if !self.collect_character(character, replaceable, start, preceding) {
                        return NeighborStop::Full;
                    }
                }
            } else {
                for character in text.chars().rev() {
                    if !self.collect_character(character, replaceable, start, preceding) {
                        return NeighborStop::Full;
                    }
                }
            }
        }
        NeighborStop::Exhausted
    }
}

// Only incoming matches are considered here. The fragment renderer handles
// matches whose complete left context fits in its four-character window.
fn consumed_dash_prefix(
    characters: &[(char, bool)],
    prefix: usize,
    boundary: bool,
    initial_skip: usize,
) -> usize {
    let mut cursor = initial_skip;
    let mut consumed = cursor.checked_sub(1);
    while cursor <= prefix && cursor + 2 < characters.len() {
        let before = cursor.checked_sub(1);
        let left = before.map_or(boundary, |index| {
            matches!(characters.get(index), Some((' ' | '\n', true))) && Some(index) != consumed
        });
        if characters.get(cursor..cursor + 2) == Some(&[('-', true), ('-', true)])
            && left
            && matches!(characters.get(cursor + 2), Some((' ' | '\n', true)))
        {
            cursor += 3;
            consumed = Some(cursor - 1);
            if cursor > prefix {
                return cursor - prefix;
            }
        } else {
            cursor += 1;
        }
    }
    0
}

#[derive(Eq, PartialEq)]
enum NeighborStop {
    Exhausted,
    Full,
    Barrier,
    LineBreak,
}

/// Apply typography [`Replacements`] to `text` only when `subs` includes
/// [`SubsFlags::REPLACEMENTS`]; otherwise borrow `text` unchanged.
///
/// `text_boundaries` records which ends of the fragment touch the surrounding
/// paragraph boundaries.
///
/// Used by the non-HTML converters (terminal, manpage) where typography is
/// applied at deeply-nested `PlainText` leaves. HTML applies typography at
/// shallow block-rendering sites and queries `subs` inline instead.
///
/// Only available when the `pre-spec-subs` feature is enabled. When the
/// feature is off, converters fall back to applying typography
/// unconditionally (the asciidoctor default).
#[cfg(feature = "pre-spec-subs")]
#[must_use]
pub fn apply_replacements<'a>(
    text: &'a str,
    subs: SubsFlags,
    replacements: &Replacements<'_>,
    text_boundaries: TextBoundaries,
) -> Cow<'a, str> {
    if subs.contains(SubsFlags::REPLACEMENTS) {
        // Quote substitutions own formatting escapes. Replacements in code or
        // quotes-disabled prose must leave those escapes intact.
        Cow::Owned(if subs.contains(SubsFlags::QUOTES) {
            replacements.transform(text, text_boundaries)
        } else {
            replacements.transform_verbatim(text, text_boundaries)
        })
    } else {
        Cow::Borrowed(text)
    }
}

// Private Use Area placeholders for escaped patterns.
// These characters won't appear in normal text and are used to protect
// escaped patterns from typography substitutions.
const ESCAPED_ELLIPSIS: &str = "\u{E000}ELLIPSIS\u{E000}";
const ESCAPED_ARROW_RIGHT: &str = "\u{E000}RARROW\u{E000}";
const ESCAPED_ARROW_LEFT: &str = "\u{E000}LARROW\u{E000}";
const ESCAPED_DARROW_RIGHT: &str = "\u{E000}RDARROW\u{E000}";
const ESCAPED_DARROW_LEFT: &str = "\u{E000}LDARROW\u{E000}";
const ESCAPED_EMDASH: &str = "\u{E000}EMDASH\u{E000}";
const ESCAPED_TRADEMARK: &str = "\u{E000}TRADEMARK\u{E000}";
const ESCAPED_COPYRIGHT: &str = "\u{E000}COPYRIGHT\u{E000}";
const ESCAPED_REGISTERED: &str = "\u{E000}REGISTERED\u{E000}";

/// Protect escaped typography patterns until [`restore_escaped_patterns`] runs.
///
/// Formatting escapes remain unchanged. `include_arrows` selects whether arrow
/// escapes participate. Callers that leave arrows literal also retain their
/// backslashes.
#[must_use]
pub fn protect_replacement_escapes(text: &str, include_arrows: bool) -> Cow<'_, str> {
    let mut text = Cow::Borrowed(text);
    if !text.contains('\\') {
        return text;
    }
    for (escaped, protected) in [
        ("\\...", ESCAPED_ELLIPSIS),
        ("\\--", ESCAPED_EMDASH),
        ("\\(TM)", ESCAPED_TRADEMARK),
        ("\\(C)", ESCAPED_COPYRIGHT),
        ("\\(R)", ESCAPED_REGISTERED),
    ] {
        text = replace_if_present(text, escaped, protected);
    }
    if include_arrows {
        for (escaped, protected) in [
            ("\\->", ESCAPED_ARROW_RIGHT),
            ("\\<-", ESCAPED_ARROW_LEFT),
            ("\\=>", ESCAPED_DARROW_RIGHT),
            ("\\<=", ESCAPED_DARROW_LEFT),
        ] {
            text = replace_if_present(text, escaped, protected);
        }
    }
    text
}

/// Apply a passthrough's own replacement profile, independent of block settings.
///
/// Replacement escapes stay literal; escapes for disabled formatting rules are
/// preserved. Source arrows require special-character substitution before
/// replacements, matching the passthrough's requested order. Authored encoded
/// arrows are recognized when an earlier escaping stage has not protected them.
#[must_use]
pub fn apply_passthrough_replacements<'a>(
    text: &'a str,
    subs: &[Substitution],
    replacements: &Replacements<'_>,
    text_boundaries: TextBoundaries,
) -> Cow<'a, str> {
    if !subs.contains(&Substitution::Replacements) {
        return Cow::Borrowed(text);
    }
    let arrows = subs
        .iter()
        .take_while(|sub| **sub != Substitution::Replacements)
        .any(|sub| *sub == Substitution::SpecialChars);
    let mut text = Cow::Borrowed(text);
    if !arrows && text.contains('&') {
        // A later escape stage sees the authored entity spelling. Without it,
        // an escaped arrow contributes literal characters to every backend.
        let escape_after = subs
            .iter()
            .skip_while(|sub| **sub != Substitution::Replacements)
            .any(|sub| *sub == Substitution::SpecialChars);
        for (pattern, replacement, literal) in [
            ("=&gt;", replacements.double_arrow_right, "=>"),
            ("-&gt;", replacements.arrow_right, "->"),
            ("&lt;=", replacements.double_arrow_left, "<="),
            ("&lt;-", replacements.arrow_left, "<-"),
        ] {
            text = replace_encoded_arrow(
                text,
                pattern,
                replacement,
                if escape_after { pattern } else { literal },
            );
        }
    }
    let raw_replacements = Replacements {
        double_arrow_right: "=>",
        double_arrow_left: "<=",
        arrow_right: "->",
        arrow_left: "<-",
        ..*replacements
    };
    let replacements = if arrows {
        replacements
    } else {
        &raw_replacements
    };
    let protected = protect_replacement_escapes(&text, arrows);
    let replaced = replacements.apply(&protected, text_boundaries);
    Cow::Owned(restore_escaped_patterns(&replaced))
}

fn replace_encoded_arrow<'a>(
    text: Cow<'a, str>,
    pattern: &str,
    replacement: &str,
    literal: &str,
) -> Cow<'a, str> {
    if !text.contains(pattern) {
        return text;
    }
    let mut result = String::with_capacity(text.len());
    let mut parts = text.split(pattern);
    let mut prefix = parts.next().unwrap_or_default();
    for next in parts {
        if let Some(prefix) = prefix.strip_suffix('\\') {
            result.push_str(prefix);
            result.push_str(literal);
        } else {
            // Right arrows run first. When they consume the shared dash or
            // equals sign, the unmatched left angle remains literal text.
            if literal != pattern
                && pattern.ends_with("&gt;")
                && let Some(prefix) = prefix.strip_suffix("&lt;")
            {
                result.push_str(prefix);
                result.push('<');
            } else {
                result.push_str(prefix);
            }
            result.push_str(replacement);
        }
        prefix = next;
    }
    result.push_str(prefix);
    Cow::Owned(result)
}

/// Remove backslash escapes from `AsciiDoc` formatting characters and patterns.
///
/// Converts formatting escapes such as `\*` → `*`.
/// Also handles multi-character pattern escapes like `\...`, `\->`, `\--`.
/// This should only be applied to non-verbatim content - verbatim contexts
/// (monospace, source blocks, literal blocks) should preserve backslashes.
///
/// # Supported escape sequences
///
/// ## Single characters (handled here)
/// - `\*` → `*` (bold marker)
/// - `\_` → `_` (italic marker)
/// - `` \` `` → `` ` `` (monospace marker)
/// - `\#` → `#` (highlight marker)
///
/// ## Single characters (handled by parser, NOT here)
/// - Bracket escapes are consumed only by active macro delimiters; otherwise literal.
/// - `\^` → context-aware (only stripped when it prevents superscript)
/// - `\~` → context-aware (only stripped when it prevents subscript)
///
/// Note: `\\` is preserved when not followed by escapable syntax (matching asciidoctor).
/// Double backslash escaping (e.g., `\\**`) is handled by the parser, not here.
///
/// ## Multi-character patterns (converted to placeholders)
/// - `\...` → placeholder (prevents ellipsis conversion)
/// - `\->` → placeholder (prevents right arrow conversion)
/// - `\<-` → placeholder (prevents left arrow conversion)
/// - `\=>` → placeholder (prevents right double arrow conversion)
/// - `\<=` → placeholder (prevents left double arrow conversion)
/// - `\--` → placeholder (prevents em-dash conversion)
/// - `\(TM)` → placeholder (prevents trademark conversion)
/// - `\(C)` → placeholder (prevents copyright conversion)
/// - `\(R)` → placeholder (prevents registered conversion)
///
/// Call [`restore_escaped_patterns`] after typography substitutions to convert
/// placeholders back to their literal forms.
///
/// # Example
///
/// ```
/// use acdc_converters_core::substitutions::strip_backslash_escapes;
///
/// assert_eq!(strip_backslash_escapes(r"\*bold\*"), "*bold*");
/// assert_eq!(strip_backslash_escapes(r"\[attr\]"), r"\[attr\]");
/// // Note: ^ and ~ escapes are handled by the parser (context-aware), not here
/// assert_eq!(strip_backslash_escapes(r"E=mc\^2"), r"E=mc\^2");
/// assert_eq!(strip_backslash_escapes(r"H\~2~O"), r"H\~2~O");
/// // Note: \\ is preserved when not followed by escapable syntax
/// assert_eq!(strip_backslash_escapes(r"path\\to\\file"), r"path\\to\\file");
/// ```
#[must_use]
pub fn strip_backslash_escapes(text: &str) -> String {
    // Fast path: nothing to do if there's no backslash in the text. This
    // skips 9+ no-op `str::replace` calls (each of which would allocate a
    // fresh `String` via copy) plus the char-by-char rebuild loop for the
    // single-character escape pass. On prose-heavy documents this path
    // fires for the overwhelming majority of text nodes.
    if !text.contains('\\') {
        return text.to_owned();
    }

    // Slow path: only rebuild strings for patterns that actually appear.
    let text = protect_replacement_escapes(text, true);

    // Then handle single-character escapes. Skip the char loop entirely if
    // the only remaining backslashes are non-escapable — saves a rebuild.
    if !text.contains('\\') {
        return text.into_owned();
    }
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        // Handle single-character escapes (excluding backslash itself).
        // Note: \\ is NOT stripped here. Per asciidoctor behavior:
        // - \\ alone or followed by non-escapable text -> preserved as \\
        // - \\** (double backslash + double marker) is handled by the parser
        //   which produces just ** in the AST, so we never see \\** here
        // Note: ^ and ~ escapes are handled by the parser (context-aware stripping).
        // They only get stripped when they actually prevented formatting (e.g., \^super^).
        // When they don't prevent anything (e.g., \^caret), the parser preserves them.
        if c == '\\'
            && chars
                .peek()
                .is_some_and(|&next| matches!(next, '*' | '_' | '`' | '#'))
        {
            // \x -> x (skip backslash, output the character)
            if let Some(escaped) = chars.next() {
                result.push(escaped);
                continue;
            }
        }
        result.push(c);
    }
    result
}

/// Replace `from` with `to` only if `from` actually occurs in `text`.
/// Avoids the unconditional `String` allocation that `str::replace` does
/// even on no-match inputs. Critical for hot-path text substitution where
/// the overwhelming majority of inputs contain none of the triggers.
fn replace_if_present<'a>(text: Cow<'a, str>, from: &str, to: &str) -> Cow<'a, str> {
    if text.contains(from) {
        Cow::Owned(text.replace(from, to))
    } else {
        text
    }
}

/// Restore escaped patterns after typography substitutions are complete.
///
/// This converts the placeholders created by [`strip_backslash_escapes`] back
/// to their literal forms. Call this after applying typography substitutions
/// (ellipsis, arrows, em-dash) to preserve the escaped patterns.
///
/// # Example
///
/// ```
/// use acdc_converters_core::substitutions::{strip_backslash_escapes, restore_escaped_patterns};
///
/// let input = r"v2.0.25\...v2.0.26";
/// let escaped = strip_backslash_escapes(input);
/// // Typography substitutions would happen here...
/// let restored = restore_escaped_patterns(&escaped);
/// assert_eq!(restored, "v2.0.25...v2.0.26");
/// ```
#[must_use]
pub fn restore_escaped_patterns(text: &str) -> String {
    // Fast path: all placeholders share the `\u{E000}` Private Use Area
    // prefix. A single contains check skips the whole chain when there's
    // nothing to restore — true for the vast majority of text nodes.
    if !text.contains('\u{E000}') {
        return text.to_owned();
    }
    let mut text = Cow::Borrowed(text);
    text = replace_if_present(text, ESCAPED_ELLIPSIS, "...");
    text = replace_if_present(text, ESCAPED_ARROW_RIGHT, "->");
    text = replace_if_present(text, ESCAPED_ARROW_LEFT, "<-");
    text = replace_if_present(text, ESCAPED_DARROW_RIGHT, "=>");
    text = replace_if_present(text, ESCAPED_DARROW_LEFT, "<=");
    text = replace_if_present(text, ESCAPED_EMDASH, "--");
    text = replace_if_present(text, ESCAPED_TRADEMARK, "(TM)");
    text = replace_if_present(text, ESCAPED_COPYRIGHT, "(C)");
    text = replace_if_present(text, ESCAPED_REGISTERED, "(R)");
    text.into_owned()
}

/// Typography replacements for `AsciiDoc` content.
///
/// Each converter provides format-specific output strings for the same set of
/// typographic patterns. Use [`Self::apply`] to transform text.
#[non_exhaustive]
pub struct Replacements<'a> {
    /// Replaces `word -- word` (em-dash with surrounding spaces) with thin-space + em-dash + thin-space.
    pub em_dash_spaced: &'a str,
    /// Replaces `word--word` (em-dash between word characters) with em-dash + zero-width-space.
    pub em_dash_word_bounded: &'a str,
    /// Replaces `=>` (rightwards double arrow).
    pub double_arrow_right: &'a str,
    /// Replaces `<=` (leftwards double arrow).
    pub double_arrow_left: &'a str,
    /// Replaces `->` (rightwards arrow).
    pub arrow_right: &'a str,
    /// Replaces `<-` (leftwards arrow).
    pub arrow_left: &'a str,
    /// Replaces `(C)` (copyright symbol).
    pub copyright: &'a str,
    /// Replaces `(R)` (registered symbol).
    pub registered: &'a str,
    /// Replaces `(TM)` (trademark symbol).
    pub trademark: &'a str,
    /// Replaces `...` (ellipsis).
    pub ellipsis: &'a str,
    /// Replaces smart apostrophes in contractions.
    pub apostrophe: &'a str,
}

impl Replacements<'static> {
    /// Format-neutral Unicode replacements.
    #[must_use]
    pub const fn unicode() -> Self {
        Self {
            em_dash_spaced: "\u{2009}\u{2014}\u{2009}",
            em_dash_word_bounded: "\u{2014}\u{200B}",
            double_arrow_right: "\u{21D2}",
            double_arrow_left: "\u{21D0}",
            arrow_right: "\u{2192}",
            arrow_left: "\u{2190}",
            copyright: "\u{00A9}",
            registered: "\u{00AE}",
            trademark: "\u{2122}",
            ellipsis: "\u{2026}",
            apostrophe: "\u{2019}",
        }
    }
}

impl Replacements<'_> {
    /// Full typography pipeline: strip escapes, apply replacements, restore escaped patterns.
    #[must_use]
    pub fn transform(&self, text: &str, text_boundaries: TextBoundaries) -> String {
        let text = strip_backslash_escapes(text);
        let text = self.apply(&text, text_boundaries);
        restore_escaped_patterns(&text)
    }

    /// Apply typography without removing unrelated formatting escapes in code.
    #[must_use]
    pub fn transform_verbatim(&self, text: &str, text_boundaries: TextBoundaries) -> String {
        let text = protect_replacement_escapes(text, true);
        let text = self.apply(&text, text_boundaries);
        restore_escaped_patterns(&text)
    }

    /// Apply typography replacements to text.
    ///
    /// Applies all `AsciiDoc` `Replacements` substitutions in the correct order:
    /// 1. Em-dashes (spaced and word-bounded patterns)
    /// 2. Double arrows before single arrows
    /// 3. Symbols: `(C)`, `(R)`, `(TM)`
    /// 4. Ellipsis: `...`
    /// 5. Smart apostrophes (context-aware)
    ///
    /// Call this on text that has already been through [`strip_backslash_escapes`],
    /// then call [`restore_escaped_patterns`] on the result.
    ///
    /// # Example
    ///
    /// ```
    /// use acdc_converters_core::substitutions::{
    ///     strip_backslash_escapes, restore_escaped_patterns, Replacements, TextBoundaries,
    /// };
    ///
    /// let text = strip_backslash_escapes("Hello -- world");
    /// let text = Replacements::unicode().apply(&text, TextBoundaries::BOTH);
    /// let text = restore_escaped_patterns(&text);
    /// assert_eq!(text, "Hello\u{2009}\u{2014}\u{2009}world");
    /// ```
    #[must_use]
    pub fn apply(&self, text: &str, text_boundaries: TextBoundaries) -> String {
        // Fast path: if none of the trigger bytes appear in the text, no
        // substitution can possibly match. `replace_em_dashes` and
        // `replace_apostrophes` still do char-by-char scans in their slow
        // paths, so we check them too — a single byte scan is much cheaper
        // than re-building the string several times.
        if !needs_substitution(text) && !text_boundaries.has_adjacent_trigger() {
            return text.to_owned();
        }

        // 1. Em-dashes
        let text = replace_em_dashes(
            text,
            self.em_dash_spaced,
            self.em_dash_word_bounded,
            text_boundaries,
        );

        // 2-4. Arrows, symbols, ellipsis — only allocate when the pattern
        // is actually present, avoiding 8 unconditional string copies for
        // the common case of text that contains none of them.
        let mut text = Cow::<str>::Owned(text);
        text = replace_if_present(text, "=>", self.double_arrow_right);
        text = replace_if_present(text, "<=", self.double_arrow_left);
        text = replace_if_present(text, "->", self.arrow_right);
        text = replace_if_present(text, "<-", self.arrow_left);
        text = replace_if_present(text, "(C)", self.copyright);
        text = replace_if_present(text, "(R)", self.registered);
        text = replace_if_present(text, "(TM)", self.trademark);
        text = replace_if_present(text, "...", self.ellipsis);

        // 5. Smart apostrophes (char-by-char rebuild; skip if no `'`).
        if text.contains('\'') || text_boundaries.has_adjacent_trigger() {
            replace_contextual_apostrophes(&text, self.apostrophe, text_boundaries)
        } else {
            text.into_owned()
        }
    }
}

/// Cheap byte-level check for any character that could start a substitution
/// trigger sequence. Used by `Replacements::apply` to skip the entire
/// transform pipeline when the text contains nothing to rewrite.
fn needs_substitution(text: &str) -> bool {
    text.as_bytes()
        .iter()
        .any(|&b| matches!(b, b'-' | b'=' | b'(' | b'\'' | b'.'))
}

/// Replace em-dash patterns in text.
///
/// Matches asciidoctor's two em-dash patterns:
/// - **Spaced**: a space or newline around `--` (or a paragraph boundary) →
///   `spaced` replacement
/// - **Word-bounded**: `\w--\w` → `word_bounded` replacement
///
/// Does NOT match: `word --word`, `word-- word`, `test--`, `--test`, `---`
///
/// # Examples
///
/// ```
/// use acdc_converters_core::substitutions::{TextBoundaries, replace_em_dashes};
///
/// // At both paragraph boundaries:
/// assert_eq!(replace_em_dashes("a -- b", "S", "W", TextBoundaries::BOTH), "aSb");
/// assert_eq!(replace_em_dashes("a--b", "S", "W", TextBoundaries::BOTH), "aWb");
/// assert_eq!(replace_em_dashes("a --b", "S", "W", TextBoundaries::BOTH), "a --b");
/// // Inside inline spans:
/// assert_eq!(replace_em_dashes("--", "S", "W", TextBoundaries::NONE), "--");
/// assert_eq!(replace_em_dashes("-- word", "S", "W", TextBoundaries::NONE), "-- word");
/// ```
#[must_use]
pub fn replace_em_dashes(
    text: &str,
    spaced: &str,
    word_bounded: &str,
    text_boundaries: TextBoundaries,
) -> String {
    let fragment = TextFragment::new(text, text_boundaries);
    let mut result = String::with_capacity(text.len());
    let mut cursor = fragment.prefix_consumed;
    let mut consumed_space = cursor.checked_sub(1).filter(|&index| {
        fragment
            .character(Some(index))
            .is_some_and(|ch| matches!(ch, ' ' | '\n'))
    });
    while cursor < fragment.characters.len() {
        if fragment.editable(cursor, '\\')
            && fragment.editable(cursor + 1, '-')
            && fragment.editable(cursor + 2, '-')
        {
            // An escape can belong to another fragment. Its owner removes the
            // backslash; each dash stays with its own source fragment.
            fragment.push_character(&mut result, cursor + 1);
            fragment.push_character(&mut result, cursor + 2);
            cursor += 3;
        } else if fragment.editable(cursor, '-')
            && fragment.editable(cursor + 1, '-')
            && fragment.character(cursor.checked_sub(1)) != Some('-')
            && fragment.character(Some(cursor + 2)) != Some('-')
        {
            let before = cursor.checked_sub(1);
            let after = cursor + 2;
            let spaced_match = fragment.space_or_boundary(before, true)
                && (before.is_none() || before != consumed_space)
                && fragment.space_or_boundary(Some(after), false);
            let word_match = fragment.character(before).is_some_and(is_word)
                && fragment.character(Some(after)).is_some_and(is_word);
            if spaced_match || word_match {
                if spaced_match && before.is_some_and(|index| fragment.local.contains(&index)) {
                    result.pop();
                }
                if fragment.local.contains(&cursor) {
                    result.push_str(if spaced_match { spaced } else { word_bounded });
                }
                cursor += 2;
                if spaced_match && fragment.character(Some(cursor)).is_some() {
                    consumed_space = Some(cursor);
                    cursor += 1;
                }
            } else {
                fragment.push_character(&mut result, cursor);
                cursor += 1;
            }
        } else {
            fragment.push_character(&mut result, cursor);
            cursor += 1;
        }
    }
    result
}

fn is_word(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// A replacement reads neighboring source characters, but only writes the
/// characters owned by its fragment. Protected neighbors supply context only.
struct TextFragment {
    characters: Vec<(char, bool)>,
    local: std::ops::Range<usize>,
    at_start: bool,
    at_end: bool,
    prefix_consumed: usize,
}

impl TextFragment {
    fn new(text: &str, boundaries: TextBoundaries) -> Self {
        let mut characters = Vec::with_capacity(text.len() + 8);
        for (index, character) in boundaries.previous.characters.iter().enumerate().rev() {
            if let Some(character) = character {
                characters.push((
                    *character,
                    boundaries.previous.replaceable & (1 << index) != 0,
                ));
            }
        }
        let start = characters.len();
        characters.extend(text.chars().map(|character| (character, true)));
        let local = start..characters.len();
        for (index, character) in boundaries.next.characters.iter().enumerate() {
            if let Some(character) = character {
                characters.push((*character, boundaries.next.replaceable & (1 << index) != 0));
            }
        }
        Self {
            characters,
            local,
            at_start: boundaries.previous.paragraph_boundary,
            at_end: boundaries.next.paragraph_boundary,
            prefix_consumed: boundaries.previous.prefix_consumed,
        }
    }

    fn character(&self, index: Option<usize>) -> Option<char> {
        index.and_then(|index| self.characters.get(index).map(|&(character, _)| character))
    }

    fn editable(&self, index: usize, expected: char) -> bool {
        self.characters.get(index) == Some(&(expected, true))
    }

    fn space_or_boundary(&self, index: Option<usize>, before: bool) -> bool {
        match index.and_then(|index| self.characters.get(index)) {
            Some((' ' | '\n', true)) => true,
            None => {
                if before {
                    self.at_start
                } else {
                    self.at_end
                }
            }
            _ => false,
        }
    }

    fn push_character(&self, output: &mut String, index: usize) {
        if self.local.contains(&index)
            && let Some(&(character, _)) = self.characters.get(index)
        {
            output.push(character);
        }
    }
}

/// Replace apostrophes between word characters with curly apostrophes.
///
/// Matches asciidoctor's replacement regex: `(\p{Alnum})\\?'(?=\p{Alpha})`
/// - Before: alphanumeric character (letters + digits)
/// - After: alphabetic character (letters only, NOT digits)
/// - Optional `\` before `'` acts as escape (strips `\`, keeps literal `'`)
///
/// # Examples
///
/// ```
/// use acdc_converters_core::substitutions::replace_apostrophes;
///
/// assert_eq!(replace_apostrophes("it's", "\u{2019}"), "it\u{2019}s");
/// assert_eq!(replace_apostrophes("3'4\"", "\u{2019}"), "3'4\"");
/// assert_eq!(replace_apostrophes("'word'", "\u{2019}"), "'word'");
/// assert_eq!(replace_apostrophes("Olaf\\'s", "\u{2019}"), "Olaf's");
/// ```
#[must_use]
pub fn replace_apostrophes(text: &str, curly_apostrophe: &str) -> String {
    replace_contextual_apostrophes(text, curly_apostrophe, TextBoundaries::NONE)
}

fn replace_contextual_apostrophes(
    text: &str,
    curly_apostrophe: &str,
    boundaries: TextBoundaries,
) -> String {
    let fragment = TextFragment::new(text, boundaries);
    let mut result = String::with_capacity(text.len());
    let mut cursor = 0;
    while cursor < fragment.characters.len() {
        let escaped = fragment.editable(cursor, '\\') && fragment.editable(cursor + 1, '\'');
        let apostrophe = cursor + usize::from(escaped);
        if fragment.editable(apostrophe, '\'')
            && fragment
                .character(cursor.checked_sub(1))
                .is_some_and(char::is_alphanumeric)
            && fragment
                .character(Some(apostrophe + 1))
                .is_some_and(char::is_alphabetic)
        {
            if fragment.local.contains(&apostrophe) {
                result.push_str(if escaped { "'" } else { curly_apostrophe });
            }
            cursor = apostrophe + 1;
        } else {
            fragment.push_character(&mut result, cursor);
            cursor += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_arrows_follow_character_escaping_order() {
        let source = r"-> => <- <= | \-> \=> \<- \<= | (C)";
        for (subs, expected) in [
            (
                vec![Substitution::SpecialChars, Substitution::Replacements],
                "→ ⇒ ← ⇐ | -> => <- <= | ©",
            ),
            (
                vec![Substitution::Replacements, Substitution::SpecialChars],
                r"-> => <- <= | \-> \=> \<- \<= | ©",
            ),
            (
                vec![Substitution::Replacements],
                r"-> => <- <= | \-> \=> \<- \<= | ©",
            ),
            (vec![Substitution::SpecialChars], source),
            (vec![], source),
        ] {
            assert_eq!(
                apply_passthrough_replacements(
                    source,
                    &subs,
                    &Replacements::unicode(),
                    TextBoundaries::BOTH
                ),
                expected,
                "{subs:?}",
            );
        }
    }

    #[test]
    fn passthrough_encoded_arrows_respect_escapes_and_prior_escaping() {
        let source = r"-&gt; =&gt; &lt;- &lt;= | \-&gt; \=&gt; \&lt;- \&lt;=";
        for (subs, expected) in [
            (vec![Substitution::Replacements], "→ ⇒ ← ⇐ | -> => <- <="),
            (
                vec![Substitution::Replacements, Substitution::SpecialChars],
                "→ ⇒ ← ⇐ | -&gt; =&gt; &lt;- &lt;=",
            ),
            (
                vec![Substitution::SpecialChars, Substitution::Replacements],
                source,
            ),
        ] {
            assert_eq!(
                apply_passthrough_replacements(
                    source,
                    &subs,
                    &Replacements::unicode(),
                    TextBoundaries::BOTH
                ),
                expected,
                "{subs:?}",
            );
        }
    }

    #[test]
    fn test_caret_escape_preserved() {
        // ^ and ~ escapes are now handled by the parser (context-aware stripping).
        // The converter preserves them as-is - the parser decides what to strip.
        assert_eq!(strip_backslash_escapes(r"\^"), r"\^");
        assert_eq!(strip_backslash_escapes(r"E=mc\^2"), r"E=mc\^2");
        assert_eq!(strip_backslash_escapes(r"\^not super^"), r"\^not super^");
    }

    #[test]
    fn test_tilde_escape_preserved() {
        // ~ escapes are now handled by the parser (context-aware stripping).
        assert_eq!(strip_backslash_escapes(r"\~"), r"\~");
        assert_eq!(strip_backslash_escapes(r"H\~2~O"), r"H\~2~O");
        assert_eq!(strip_backslash_escapes(r"\~not sub~"), r"\~not sub~");
    }

    #[test]
    fn test_strip_other_escapes() {
        assert_eq!(strip_backslash_escapes(r"\*bold\*"), "*bold*");
        assert_eq!(strip_backslash_escapes(r"\_italic\_"), "_italic_");
        assert_eq!(strip_backslash_escapes(r"\`mono\`"), "`mono`");
        assert_eq!(strip_backslash_escapes(r"\#marked\#"), "#marked#");
        // Note: \\ is now preserved per asciidoctor behavior (double backslash
        // escaping is handled by the parser, not the converter)
        assert_eq!(strip_backslash_escapes(r"\\"), r"\\");
        assert_eq!(strip_backslash_escapes(r"\[attr\]"), r"\[attr\]");
    }

    #[test]
    fn test_preserves_other_backslashes() {
        // Backslashes not followed by escapable chars are preserved
        assert_eq!(strip_backslash_escapes(r"\n"), r"\n");
        assert_eq!(strip_backslash_escapes(r"C:\path"), r"C:\path");
        // Single dot is NOT escapable (backslash preserved)
        assert_eq!(strip_backslash_escapes(r"a\.b"), r"a\.b");
    }

    #[test]
    fn test_empty_and_no_escapes() {
        assert_eq!(strip_backslash_escapes(""), "");
        assert_eq!(strip_backslash_escapes("plain text"), "plain text");
    }

    #[test]
    fn test_strip_pattern_escapes() {
        // Ellipsis escape - uses placeholder
        assert_eq!(strip_backslash_escapes(r"\..."), ESCAPED_ELLIPSIS);
        assert!(strip_backslash_escapes(r"v2.0.25\...v2.0.26").contains(ESCAPED_ELLIPSIS));

        // Arrow escapes - use placeholders
        assert_eq!(strip_backslash_escapes(r"\->"), ESCAPED_ARROW_RIGHT);
        assert_eq!(strip_backslash_escapes(r"\<-"), ESCAPED_ARROW_LEFT);
        assert_eq!(strip_backslash_escapes(r"\=>"), ESCAPED_DARROW_RIGHT);
        assert_eq!(strip_backslash_escapes(r"\<="), ESCAPED_DARROW_LEFT);
        assert_eq!(strip_backslash_escapes(r"\--"), ESCAPED_EMDASH);
        assert_eq!(strip_backslash_escapes(r"\(TM)"), ESCAPED_TRADEMARK);
        assert_eq!(strip_backslash_escapes(r"\(C)"), ESCAPED_COPYRIGHT);
        assert_eq!(strip_backslash_escapes(r"\(R)"), ESCAPED_REGISTERED);
    }

    #[test]
    fn test_restore_escaped_patterns() {
        assert_eq!(restore_escaped_patterns(ESCAPED_ELLIPSIS), "...");
        assert_eq!(restore_escaped_patterns(ESCAPED_ARROW_RIGHT), "->");
        assert_eq!(restore_escaped_patterns(ESCAPED_ARROW_LEFT), "<-");
        assert_eq!(restore_escaped_patterns(ESCAPED_DARROW_RIGHT), "=>");
        assert_eq!(restore_escaped_patterns(ESCAPED_DARROW_LEFT), "<=");
        assert_eq!(restore_escaped_patterns(ESCAPED_EMDASH), "--");
        assert_eq!(restore_escaped_patterns(ESCAPED_TRADEMARK), "(TM)");
        assert_eq!(restore_escaped_patterns(ESCAPED_COPYRIGHT), "(C)");
        assert_eq!(restore_escaped_patterns(ESCAPED_REGISTERED), "(R)");
    }

    #[test]
    fn test_roundtrip_escape_restore() {
        let input = r"v2.0.25\...v2.0.26";
        let escaped = strip_backslash_escapes(input);
        let restored = restore_escaped_patterns(&escaped);
        assert_eq!(restored, "v2.0.25...v2.0.26");
    }

    #[test]
    fn test_roundtrip_arrows() {
        // Test that escaped arrows survive the roundtrip
        assert_eq!(
            restore_escaped_patterns(&strip_backslash_escapes(r"use \-> instead")),
            "use -> instead"
        );
        assert_eq!(
            restore_escaped_patterns(&strip_backslash_escapes(r"\<- back")),
            "<- back"
        );
    }

    // --- apply_replacements tests ---

    #[cfg(feature = "pre-spec-subs")]
    #[test]
    fn replacements_leave_formatting_escapes_to_quotes() {
        assert_eq!(
            apply_replacements(
                r"(C) \*literal\*",
                SubsFlags::REPLACEMENTS,
                &UNICODE,
                TextBoundaries::BOTH
            ),
            r"© \*literal\*"
        );
        assert_eq!(
            apply_replacements(
                r"(C) \*literal\*",
                SubsFlags::REPLACEMENTS | SubsFlags::QUOTES,
                &UNICODE,
                TextBoundaries::BOTH
            ),
            "© *literal*"
        );
    }

    const UNICODE: Replacements<'static> = Replacements::unicode();

    #[test]
    fn test_em_dash_spaced() {
        assert_eq!(
            UNICODE.apply("a -- b", TextBoundaries::BOTH),
            "a\u{2009}\u{2014}\u{2009}b"
        );
    }

    #[test]
    fn test_em_dash_at_start() {
        assert_eq!(
            UNICODE.apply("-- b", TextBoundaries::BOTH),
            "\u{2009}\u{2014}\u{2009}b"
        );
    }

    #[test]
    fn test_em_dash_at_end() {
        assert_eq!(
            UNICODE.apply("a --", TextBoundaries::BOTH),
            "a\u{2009}\u{2014}\u{2009}"
        );
    }

    #[test]
    fn test_em_dash_word_bounded() {
        assert_eq!(
            UNICODE.apply("word--word", TextBoundaries::BOTH),
            "word\u{2014}\u{200B}word"
        );
    }

    #[test]
    fn test_em_dash_digit_bounded() {
        assert_eq!(
            UNICODE.apply("1--2", TextBoundaries::BOTH),
            "1\u{2014}\u{200B}2"
        );
    }

    #[test]
    fn test_em_dash_no_match_left_space() {
        // space before, word after: no match
        assert_eq!(
            UNICODE.apply("word --word", TextBoundaries::BOTH),
            "word --word"
        );
    }

    #[test]
    fn test_em_dash_no_match_right_space() {
        // word before, space after: no match
        assert_eq!(
            UNICODE.apply("word-- word", TextBoundaries::BOTH),
            "word-- word"
        );
    }

    #[test]
    fn test_em_dash_no_match_trailing() {
        // word before, end of string: no match
        assert_eq!(UNICODE.apply("test--", TextBoundaries::BOTH), "test--");
    }

    #[test]
    fn test_em_dash_no_match_leading() {
        // start of string, word after: no match
        assert_eq!(UNICODE.apply("--test", TextBoundaries::BOTH), "--test");
    }

    #[test]
    fn test_em_dash_triple_dash_no_match() {
        assert_eq!(UNICODE.apply("---", TextBoundaries::BOTH), "---");
    }

    #[test]
    fn test_double_arrow_right() {
        assert_eq!(
            UNICODE.apply("a => b", TextBoundaries::BOTH),
            "a \u{21D2} b"
        );
    }

    #[test]
    fn test_double_arrow_left() {
        assert_eq!(
            UNICODE.apply("a <= b", TextBoundaries::BOTH),
            "a \u{21D0} b"
        );
    }

    #[test]
    fn test_arrow_right() {
        assert_eq!(
            UNICODE.apply("a -> b", TextBoundaries::BOTH),
            "a \u{2192} b"
        );
    }

    #[test]
    fn test_arrow_left() {
        assert_eq!(
            UNICODE.apply("a <- b", TextBoundaries::BOTH),
            "a \u{2190} b"
        );
    }

    #[test]
    fn test_double_arrow_before_single() {
        // => must be matched before -> to avoid partial match
        assert_eq!(
            UNICODE.apply("a => b -> c", TextBoundaries::BOTH),
            "a \u{21D2} b \u{2192} c"
        );
    }

    #[test]
    fn test_copyright() {
        assert_eq!(
            UNICODE.apply("(C) 2024", TextBoundaries::BOTH),
            "\u{00A9} 2024"
        );
    }

    #[test]
    fn test_registered() {
        assert_eq!(UNICODE.apply("Foo(R)", TextBoundaries::BOTH), "Foo\u{00AE}");
    }

    #[test]
    fn test_trademark() {
        assert_eq!(
            UNICODE.apply("Foo(TM)", TextBoundaries::BOTH),
            "Foo\u{2122}"
        );
    }

    #[test]
    fn test_ellipsis() {
        assert_eq!(
            UNICODE.apply("wait...", TextBoundaries::BOTH),
            "wait\u{2026}"
        );
    }

    #[test]
    fn test_apostrophe_contraction() {
        assert_eq!(
            UNICODE.apply("it's great", TextBoundaries::BOTH),
            "it\u{2019}s great"
        );
    }

    #[test]
    fn test_apostrophe_digit_after_not_converted() {
        assert_eq!(UNICODE.apply("3'4\"", TextBoundaries::BOTH), "3'4\"");
    }

    #[test]
    fn test_apostrophe_quotes_not_converted() {
        assert_eq!(UNICODE.apply("'word'", TextBoundaries::BOTH), "'word'");
    }

    #[test]
    fn test_apostrophe_escaped() {
        assert_eq!(UNICODE.apply("Olaf\\'s", TextBoundaries::BOTH), "Olaf's");
    }

    #[test]
    fn test_apostrophe_decade() {
        assert_eq!(
            UNICODE.apply("1990's", TextBoundaries::BOTH),
            "1990\u{2019}s"
        );
    }

    #[test]
    fn test_all_replacements_combined() {
        assert_eq!(
            UNICODE.apply("(C) 2024 -- it's cool...", TextBoundaries::BOTH),
            "\u{00A9} 2024\u{2009}\u{2014}\u{2009}it\u{2019}s cool\u{2026}"
        );
    }

    #[test]
    fn test_no_replacements() {
        assert_eq!(
            UNICODE.apply("plain text", TextBoundaries::BOTH),
            "plain text"
        );
    }

    #[test]
    fn verbatim_replacements_preserve_formatting_escapes() {
        assert_eq!(
            UNICODE.transform_verbatim(
                r"(C) \(R) \(TM) \... word\--word \-> \<- \=> \<= \*bold\* \_italic\_ \`code\` \#mark\# \^up\^ \~down\~",
                TextBoundaries::BOTH,
            ),
            r"© (R) (TM) ... word--word -> <- => <= \*bold\* \_italic\_ \`code\` \#mark\# \^up\^ \~down\~"
        );
        assert_eq!(
            UNICODE.transform_verbatim("-- word --", TextBoundaries::NONE),
            "-- word --"
        );
        assert_eq!(
            UNICODE.transform_verbatim("-- word --", TextBoundaries::BOTH),
            "\u{2009}—\u{2009}word\u{2009}—\u{2009}"
        );
    }

    #[test]
    fn test_full_pipeline_with_escapes() {
        let input = r"Hello \-- world -- done";
        let text = strip_backslash_escapes(input);
        let text = UNICODE.apply(&text, TextBoundaries::BOTH);
        let text = restore_escaped_patterns(&text);
        assert_eq!(text, "Hello -- world\u{2009}\u{2014}\u{2009}done");
    }

    // --- string_boundaries_are_space=false tests (inline span context) ---

    #[test]
    fn test_em_dash_inline_span_standalone() {
        // Inside inline spans, bare "--" should NOT become em-dash
        assert_eq!(UNICODE.apply("--", TextBoundaries::NONE), "--");
    }

    #[test]
    fn test_em_dash_inline_span_leading() {
        // "-- word" at start of inline span: no em-dash
        assert_eq!(UNICODE.apply("-- word", TextBoundaries::NONE), "-- word");
    }

    #[test]
    fn test_em_dash_inline_span_trailing() {
        // "word --" at end of inline span: no em-dash
        assert_eq!(UNICODE.apply("word --", TextBoundaries::NONE), "word --");
    }

    #[test]
    fn test_em_dash_inline_span_spaced_middle() {
        // "word -- word" still works (actual space chars on both sides)
        assert_eq!(
            UNICODE.apply("word -- word", TextBoundaries::NONE),
            "word\u{2009}\u{2014}\u{2009}word"
        );
    }

    #[test]
    fn test_em_dash_inline_span_word_bounded() {
        // "word--word" still works (unaffected by boundary flag)
        assert_eq!(
            UNICODE.apply("word--word", TextBoundaries::NONE),
            "word\u{2014}\u{200B}word"
        );
    }

    #[test]
    fn test_em_dash_fragment_at_paragraph_start_only() {
        assert_eq!(
            UNICODE.apply("-- word --", TextBoundaries::new(true, false)),
            "\u{2009}\u{2014}\u{2009}word --"
        );
    }

    #[test]
    fn test_em_dash_fragment_at_paragraph_end_only() {
        assert_eq!(
            UNICODE.apply("-- word --", TextBoundaries::new(false, true)),
            "-- word\u{2009}\u{2014}\u{2009}"
        );
    }

    #[test]
    fn test_spaced_em_dash_consumes_one_space_on_each_side() {
        assert_eq!(
            UNICODE.apply("word  --  word", TextBoundaries::BOTH),
            "word \u{2009}\u{2014}\u{2009} word"
        );
    }

    #[test]
    fn test_spaced_em_dash_consumes_one_newline_on_each_side() {
        assert_eq!(
            UNICODE.apply("word\n\n--\n\nword", TextBoundaries::BOTH),
            "word\n\u{2009}\u{2014}\u{2009}\nword"
        );
    }

    #[test]
    fn test_tab_does_not_bound_spaced_em_dash() {
        assert_eq!(
            UNICODE.apply("word\t--\tword", TextBoundaries::BOTH),
            "word\t--\tword"
        );
    }
}
