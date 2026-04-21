//! Text substitution utilities for `AsciiDoc` converters.
//!
//! This module provides functions for processing `AsciiDoc` text substitutions
//! that are common across different output formats (HTML, terminal, etc.).

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

/// Remove backslash escapes from `AsciiDoc` formatting characters and patterns.
///
/// Converts escape sequences like `\*` → `*`, `\[` → `[`, etc.
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
/// - `\[` → `[` (attribute/macro opener)
/// - `\]` → `]` (attribute/macro closer)
///
/// ## Single characters (handled by parser, NOT here)
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
/// assert_eq!(strip_backslash_escapes(r"\[attr\]"), "[attr]");
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
    let mut text = std::borrow::Cow::Borrowed(text);
    text = replace_if_present(text, "\\...", ESCAPED_ELLIPSIS);
    text = replace_if_present(text, "\\->", ESCAPED_ARROW_RIGHT);
    text = replace_if_present(text, "\\<-", ESCAPED_ARROW_LEFT);
    text = replace_if_present(text, "\\=>", ESCAPED_DARROW_RIGHT);
    text = replace_if_present(text, "\\<=", ESCAPED_DARROW_LEFT);
    text = replace_if_present(text, "\\--", ESCAPED_EMDASH);
    text = replace_if_present(text, "\\(TM)", ESCAPED_TRADEMARK);
    text = replace_if_present(text, "\\(C)", ESCAPED_COPYRIGHT);
    text = replace_if_present(text, "\\(R)", ESCAPED_REGISTERED);

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
                .is_some_and(|&next| matches!(next, '*' | '_' | '`' | '#' | '[' | ']'))
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
fn replace_if_present<'a>(
    text: std::borrow::Cow<'a, str>,
    from: &str,
    to: &str,
) -> std::borrow::Cow<'a, str> {
    if text.contains(from) {
        std::borrow::Cow::Owned(text.replace(from, to))
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
    let mut text = std::borrow::Cow::Borrowed(text);
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
    /// Unicode replacements for terminal, manpage, and other non-HTML converters.
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

    /// HTML entity replacements for the HTML converter.
    #[must_use]
    pub const fn html() -> Self {
        Self {
            em_dash_spaced: "&thinsp;&mdash;&thinsp;",
            em_dash_word_bounded: "&#8212;&#8203;",
            double_arrow_right: "&#8658;",
            double_arrow_left: "&#8656;",
            arrow_right: "&#8594;",
            arrow_left: "&#8592;",
            copyright: "&#169;",
            registered: "&#174;",
            trademark: "&#8482;",
            ellipsis: "&#8230;&#8203;",
            apostrophe: "&#8217;",
        }
    }
}

impl Replacements<'_> {
    /// Full typography pipeline: strip escapes, apply replacements, restore escaped patterns.
    #[must_use]
    pub fn transform(&self, text: &str, string_boundaries_are_space: bool) -> String {
        let text = strip_backslash_escapes(text);
        let text = self.apply(&text, string_boundaries_are_space);
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
    ///     strip_backslash_escapes, restore_escaped_patterns, Replacements,
    /// };
    ///
    /// let text = strip_backslash_escapes("Hello -- world");
    /// let text = Replacements::unicode().apply(&text, true);
    /// let text = restore_escaped_patterns(&text);
    /// assert_eq!(text, "Hello\u{2009}\u{2014}\u{2009}world");
    /// ```
    #[must_use]
    pub fn apply(&self, text: &str, string_boundaries_are_space: bool) -> String {
        // Fast path: if none of the trigger bytes appear in the text, no
        // substitution can possibly match. `replace_em_dashes` and
        // `replace_apostrophes` still do char-by-char scans in their slow
        // paths, so we check them too — a single byte scan is much cheaper
        // than re-building the string several times.
        if !needs_substitution(text) {
            return text.to_owned();
        }

        // 1. Em-dashes
        let text = replace_em_dashes(
            text,
            self.em_dash_spaced,
            self.em_dash_word_bounded,
            string_boundaries_are_space,
        );

        // 2-4. Arrows, symbols, ellipsis — only allocate when the pattern
        // is actually present, avoiding 8 unconditional string copies for
        // the common case of text that contains none of them.
        let mut text = std::borrow::Cow::<str>::Owned(text);
        text = replace_if_present(text, "=>", self.double_arrow_right);
        text = replace_if_present(text, "<=", self.double_arrow_left);
        text = replace_if_present(text, "->", self.arrow_right);
        text = replace_if_present(text, "<-", self.arrow_left);
        text = replace_if_present(text, "(C)", self.copyright);
        text = replace_if_present(text, "(R)", self.registered);
        text = replace_if_present(text, "(TM)", self.trademark);
        text = replace_if_present(text, "...", self.ellipsis);

        // 5. Smart apostrophes (char-by-char rebuild; skip if no `'`).
        if text.contains('\'') {
            replace_apostrophes(&text, self.apostrophe)
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
/// - **Spaced**: `\s--\s` (or at start/end of string) → `spaced` replacement
/// - **Word-bounded**: `\w--\w` → `word_bounded` replacement
///
/// Does NOT match: `word --word`, `word-- word`, `test--`, `--test`, `---`
///
/// # Examples
///
/// ```
/// use acdc_converters_core::substitutions::replace_em_dashes;
///
/// // With string_boundaries_are_space=true (paragraph-level text):
/// assert_eq!(replace_em_dashes("a -- b", "S", "W", true), "aSb");
/// assert_eq!(replace_em_dashes("a--b", "S", "W", true), "aWb");
/// assert_eq!(replace_em_dashes("a --b", "S", "W", true), "a --b");
/// // With string_boundaries_are_space=false (inside inline spans):
/// assert_eq!(replace_em_dashes("--", "S", "W", false), "--");
/// assert_eq!(replace_em_dashes("-- word", "S", "W", false), "-- word");
/// ```
#[must_use]
pub fn replace_em_dashes(
    text: &str,
    spaced: &str,
    word_bounded: &str,
    string_boundaries_are_space: bool,
) -> String {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;

    while i < len {
        let Some(&ch) = chars.get(i) else { break };

        // Check for exactly two dashes (not three or more)
        if ch == '-'
            && chars.get(i + 1) == Some(&'-')
            && chars.get(i + 2) != Some(&'-')
            && (i == 0 || chars.get(i - 1) != Some(&'-'))
        {
            let prev = if i > 0 {
                chars.get(i - 1).copied()
            } else {
                None
            };
            let next = chars.get(i + 2).copied();

            let prev_is_space = (string_boundaries_are_space && prev.is_none())
                || prev.is_some_and(char::is_whitespace);
            let next_is_space = (string_boundaries_are_space && next.is_none())
                || next.is_some_and(char::is_whitespace);

            if prev_is_space && next_is_space {
                // Spaced em-dash: consume surrounding whitespace
                let trimmed_len = result.trim_end_matches(char::is_whitespace).len();
                result.truncate(trimmed_len);
                result.push_str(spaced);
                // Skip the two dashes
                i += 2;
                // Skip following whitespace
                while chars.get(i).is_some_and(|c| c.is_whitespace()) {
                    i += 1;
                }
                continue;
            }

            let prev_is_word = prev.is_some_and(|c| c.is_alphanumeric() || c == '_');
            let next_is_word = next.is_some_and(|c| c.is_alphanumeric() || c == '_');

            if prev_is_word && next_is_word {
                // Word-bounded em-dash
                result.push_str(word_bounded);
                i += 2;
                continue;
            }
        }

        result.push(ch);
        i += 1;
    }

    result
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
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;

    while i < chars.len() {
        let Some(&c) = chars.get(i) else {
            break;
        };

        if c == '\\' && chars.get(i + 1) == Some(&'\'') {
            // Possible escaped apostrophe: alnum\'+alpha
            let prev_is_alnum = i > 0 && chars.get(i - 1).is_some_and(|ch| ch.is_alphanumeric());
            let next_is_alpha = chars.get(i + 2).is_some_and(|ch| ch.is_alphabetic());
            if prev_is_alnum && next_is_alpha {
                // Escaped apostrophe: strip \, output literal '
                result.push('\'');
                i += 2;
                continue;
            }
            result.push(c);
        } else if c == '\'' {
            let prev_is_alnum = i > 0 && chars.get(i - 1).is_some_and(|ch| ch.is_alphanumeric());
            let next_is_alpha = chars.get(i + 1).is_some_and(|ch| ch.is_alphabetic());
            if prev_is_alnum && next_is_alpha {
                result.push_str(curly_apostrophe);
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
        i += 1;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(strip_backslash_escapes(r"\[attr\]"), "[attr]");
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

    const UNICODE: Replacements<'static> = Replacements::unicode();

    #[test]
    fn test_em_dash_spaced() {
        assert_eq!(UNICODE.apply("a -- b", true), "a\u{2009}\u{2014}\u{2009}b");
    }

    #[test]
    fn test_em_dash_at_start() {
        assert_eq!(UNICODE.apply("-- b", true), "\u{2009}\u{2014}\u{2009}b");
    }

    #[test]
    fn test_em_dash_at_end() {
        assert_eq!(UNICODE.apply("a --", true), "a\u{2009}\u{2014}\u{2009}");
    }

    #[test]
    fn test_em_dash_word_bounded() {
        assert_eq!(
            UNICODE.apply("word--word", true),
            "word\u{2014}\u{200B}word"
        );
    }

    #[test]
    fn test_em_dash_digit_bounded() {
        assert_eq!(UNICODE.apply("1--2", true), "1\u{2014}\u{200B}2");
    }

    #[test]
    fn test_em_dash_no_match_left_space() {
        // space before, word after: no match
        assert_eq!(UNICODE.apply("word --word", true), "word --word");
    }

    #[test]
    fn test_em_dash_no_match_right_space() {
        // word before, space after: no match
        assert_eq!(UNICODE.apply("word-- word", true), "word-- word");
    }

    #[test]
    fn test_em_dash_no_match_trailing() {
        // word before, end of string: no match
        assert_eq!(UNICODE.apply("test--", true), "test--");
    }

    #[test]
    fn test_em_dash_no_match_leading() {
        // start of string, word after: no match
        assert_eq!(UNICODE.apply("--test", true), "--test");
    }

    #[test]
    fn test_em_dash_triple_dash_no_match() {
        assert_eq!(UNICODE.apply("---", true), "---");
    }

    #[test]
    fn test_double_arrow_right() {
        assert_eq!(UNICODE.apply("a => b", true), "a \u{21D2} b");
    }

    #[test]
    fn test_double_arrow_left() {
        assert_eq!(UNICODE.apply("a <= b", true), "a \u{21D0} b");
    }

    #[test]
    fn test_arrow_right() {
        assert_eq!(UNICODE.apply("a -> b", true), "a \u{2192} b");
    }

    #[test]
    fn test_arrow_left() {
        assert_eq!(UNICODE.apply("a <- b", true), "a \u{2190} b");
    }

    #[test]
    fn test_double_arrow_before_single() {
        // => must be matched before -> to avoid partial match
        assert_eq!(
            UNICODE.apply("a => b -> c", true),
            "a \u{21D2} b \u{2192} c"
        );
    }

    #[test]
    fn test_copyright() {
        assert_eq!(UNICODE.apply("(C) 2024", true), "\u{00A9} 2024");
    }

    #[test]
    fn test_registered() {
        assert_eq!(UNICODE.apply("Foo(R)", true), "Foo\u{00AE}");
    }

    #[test]
    fn test_trademark() {
        assert_eq!(UNICODE.apply("Foo(TM)", true), "Foo\u{2122}");
    }

    #[test]
    fn test_ellipsis() {
        assert_eq!(UNICODE.apply("wait...", true), "wait\u{2026}");
    }

    #[test]
    fn test_apostrophe_contraction() {
        assert_eq!(UNICODE.apply("it's great", true), "it\u{2019}s great");
    }

    #[test]
    fn test_apostrophe_digit_after_not_converted() {
        assert_eq!(UNICODE.apply("3'4\"", true), "3'4\"");
    }

    #[test]
    fn test_apostrophe_quotes_not_converted() {
        assert_eq!(UNICODE.apply("'word'", true), "'word'");
    }

    #[test]
    fn test_apostrophe_escaped() {
        assert_eq!(UNICODE.apply("Olaf\\'s", true), "Olaf's");
    }

    #[test]
    fn test_apostrophe_decade() {
        assert_eq!(UNICODE.apply("1990's", true), "1990\u{2019}s");
    }

    #[test]
    fn test_all_replacements_combined() {
        assert_eq!(
            UNICODE.apply("(C) 2024 -- it's cool...", true),
            "\u{00A9} 2024\u{2009}\u{2014}\u{2009}it\u{2019}s cool\u{2026}"
        );
    }

    #[test]
    fn test_no_replacements() {
        assert_eq!(UNICODE.apply("plain text", true), "plain text");
    }

    #[test]
    fn test_full_pipeline_with_escapes() {
        let input = r"Hello \-- world -- done";
        let text = strip_backslash_escapes(input);
        let text = UNICODE.apply(&text, true);
        let text = restore_escaped_patterns(&text);
        assert_eq!(text, "Hello -- world\u{2009}\u{2014}\u{2009}done");
    }

    // --- string_boundaries_are_space=false tests (inline span context) ---

    #[test]
    fn test_em_dash_inline_span_standalone() {
        // Inside inline spans, bare "--" should NOT become em-dash
        assert_eq!(UNICODE.apply("--", false), "--");
    }

    #[test]
    fn test_em_dash_inline_span_leading() {
        // "-- word" at start of inline span: no em-dash
        assert_eq!(UNICODE.apply("-- word", false), "-- word");
    }

    #[test]
    fn test_em_dash_inline_span_trailing() {
        // "word --" at end of inline span: no em-dash
        assert_eq!(UNICODE.apply("word --", false), "word --");
    }

    #[test]
    fn test_em_dash_inline_span_spaced_middle() {
        // "word -- word" still works (actual space chars on both sides)
        assert_eq!(
            UNICODE.apply("word -- word", false),
            "word\u{2009}\u{2014}\u{2009}word"
        );
    }

    #[test]
    fn test_em_dash_inline_span_word_bounded() {
        // "word--word" still works (unaffected by boundary flag)
        assert_eq!(
            UNICODE.apply("word--word", false),
            "word\u{2014}\u{200B}word"
        );
    }
}
