//! Line-comment adjacency tracking for the preprocessor.
//!
//! asciidoctor's reader removes `//` line comments. A comment sitting directly
//! against preceding block content (no blank line) would otherwise be absorbed
//! into that paragraph's text, so it is dropped to match. A comment that stands
//! alone (preceded by a blank line, a title, a `+` continuation marker, or
//! another comment) is preserved so the grammar can model it as a `Comment`
//! block. With Setext titles enabled, the underline beneath a two-line title is
//! a heading boundary too. Comments inside verbatim blocks are preserved
//! verbatim, and `tag::` / `end::` directives are kept for include tag filtering.
//!
//! [`CommentScanner`] holds the running line-to-line context that decision
//! needs (the open verbatim block and whether the previous line was content a
//! comment would attach to). It is shared by the two callers so the rule lives
//! in one place: [`super::Preprocessor::process_inner`] actually drops the
//! comments while rebuilding the text, and
//! [`super::Preprocessor::try_pass_through`] replays the same decision to detect
//! whether that rebuild would change anything (and so can be skipped).

use super::Preprocessor;
use crate::Options;

/// Whether Setext (two-line, underlined) titles are active: the `setext` feature
/// is compiled in *and* the runtime option is set. Mirrors
/// [`crate::grammar::setext::is_enabled`], but reads the flag from [`Options`]
/// (the preprocessor runs before a `ParserState` exists).
#[cfg(feature = "setext")]
pub(super) fn setext_enabled(options: &Options) -> bool {
    options.setext
}

#[cfg(not(feature = "setext"))]
pub(super) fn setext_enabled(_options: &Options) -> bool {
    false
}

/// A `//`-prefixed line comment, but not `///` (literal text) or `////` (a
/// block-comment delimiter). Un-trimmed on purpose: an indented `  //` is a
/// literal/indented line, not a comment.
fn is_line_comment(line: &str) -> bool {
    line.starts_with("//") && !line.starts_with("///")
}

/// A Setext underline: a uniform run of a single Setext underline character
/// (`=`, `-`, `~`, `^`, `+`). The char set comes from
/// [`crate::grammar::setext::char_to_level`], which yields `None` (so this is
/// always `false`) when the `setext` feature is not compiled in.
fn is_setext_underline(line: &str) -> bool {
    let mut chars = line.chars();
    match chars.next() {
        Some(first) => {
            crate::grammar::setext::char_to_level(first).is_some() && chars.all(|c| c == first)
        }
        None => false,
    }
}

/// A document or section title — a block boundary rather than paragraph content,
/// so an adjacent line comment after it is preserved rather than dropped. Covers
/// ATX titles (`=`, `==`, … followed by a space) always, and, when `setext` is
/// enabled, the underline beneath a two-line title.
fn is_title_line(line: &str, setext: bool) -> bool {
    let rest = line.trim_start_matches('=');
    if rest.len() < line.len() && rest.starts_with(' ') {
        return true;
    }
    setext && is_setext_underline(line)
}

/// Whether the previous emitted line is paragraph-ish content that a following
/// adjacent line comment would be absorbed into — the inverse of the boundary
/// set (blank line / line comment / lone `+` continuation marker / title).
fn is_attaching_content(line: &str, setext: bool) -> bool {
    !line.trim().is_empty()
        && !is_line_comment(line)
        && line.trim() != "+"
        && !is_title_line(line, setext)
}

/// Running line context for deciding whether an adjacent `//` line comment
/// should be dropped (see the module docs for the rule).
pub(super) struct CommentScanner<'a> {
    /// The open verbatim/raw block delimiter, or `None` when outside one
    /// (so "inside a verbatim block" is `verbatim.is_some()`).
    verbatim: Option<&'a str>,
    /// Whether the previous emitted line is content a comment would merge into.
    prev_attaches: bool,
    /// Whether Setext titles are active, so their underlines count as a heading
    /// boundary (see [`setext_enabled`]).
    setext: bool,
}

impl<'a> CommentScanner<'a> {
    /// Start as if at a blank boundary so a leading comment is preserved.
    pub(super) fn new(setext: bool) -> Self {
        Self {
            verbatim: None,
            prev_attaches: false,
            setext,
        }
    }

    /// Whether `line` is a verbatim/raw block delimiter (the caller must then
    /// push and [`record`](Self::record) it). Toggles the block open/closed; a
    /// non-matching delimiter seen while already inside a block is left as-is
    /// but still reported as a delimiter line.
    pub(super) fn at_verbatim_delimiter(&mut self, line: &'a str) -> bool {
        match Preprocessor::is_verbatim_delimiter(line) {
            Some(delimiter) if self.verbatim == Some(delimiter) => {
                self.verbatim = None;
                true
            }
            Some(delimiter) if self.verbatim.is_none() => {
                self.verbatim = Some(delimiter);
                true
            }
            Some(_) => true,
            None => false,
        }
    }

    /// Whether `line` is an adjacent line comment the reader drops. Pure
    /// (`&self`): the drop path must not mutate state, so a run of adjacent
    /// comments is dropped together.
    pub(super) fn drops(&self, line: &str) -> bool {
        self.verbatim.is_none()
            && is_line_comment(line)
            && !super::tag::is_tag_directive_line(line)
            && self.prev_attaches
    }

    /// Record an emitted `line` so the next decision sees the correct context.
    pub(super) fn record(&mut self, line: &str) {
        self.prev_attaches = is_attaching_content(line, self.setext);
    }
}
