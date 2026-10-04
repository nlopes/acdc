//! Comment context tracking for the preprocessor.
//!
//! `////` block comments bypass preprocessing on both paths. Other verbatim
//! blocks keep slash lines literal while still processing directives.
//! Tables are buffered before their cell styles are parsed, so their slash
//! lines do not start comment blocks during outer preprocessing.
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
//! [`CommentScanner`] holds the open comment/verbatim block and the preceding
//! line context. It is shared by the two callers so the rule lives
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

// Valid delimiters use one leading byte and a uniform remainder. Keeping their
// byte and length lets block context cross include buffers without borrowing them.
fn delimiter_key(line: &str) -> Option<(u8, usize)> {
    line.bytes().next().map(|byte| (byte, line.len()))
}

/// Open block delimiters shared across include boundaries.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct BlockContext {
    /// The open verbatim/raw block delimiter, or `None` when outside one
    /// (so "inside a verbatim block" is `verbatim.is_some()`).
    verbatim: Option<(u8, usize)>,
    table: Option<(u8, usize)>,
    block_comment: Option<(u8, usize)>,
}

/// Running block and line context for the comment rules in the module docs.
pub(super) struct CommentScanner {
    blocks: BlockContext,
    /// Whether the previous emitted line is content a comment would merge into.
    prev_attaches: bool,
    /// Whether Setext titles are active, so their underlines count as a heading
    /// boundary (see [`setext_enabled`]).
    setext: bool,
}

impl CommentScanner {
    /// Start as if at a blank boundary so a leading comment is preserved.
    pub(super) fn new(setext: bool, blocks: BlockContext) -> Self {
        Self {
            blocks,
            prev_attaches: false,
            setext,
        }
    }

    /// Outside table buffers, whether `line` is a verbatim/raw block delimiter.
    /// The caller must then push and [`record`](Self::record) it. A non-matching
    /// delimiter inside a block leaves it open but is still reported as a
    /// delimiter line.
    pub(super) fn at_verbatim_delimiter(&mut self, line: &str) -> bool {
        if self.blocks.table.is_some() {
            return false;
        }
        match Preprocessor::is_verbatim_delimiter(line).and_then(delimiter_key) {
            Some(delimiter) if self.blocks.verbatim == Some(delimiter) => {
                self.blocks.verbatim = None;
                true
            }
            Some(delimiter) if self.blocks.verbatim.is_none() => {
                self.blocks.verbatim = Some(delimiter);
                true
            }
            Some(_) => true,
            None => false,
        }
    }

    /// Track the outer table fence without interpreting its cell contents.
    /// `AsciiDoc` cell comments are handled when the buffered cells are parsed.
    pub(super) fn at_table_delimiter(&mut self, line: &str) -> bool {
        if let Some(delimiter) = self.blocks.table {
            if delimiter_key(line) == Some(delimiter)
                && line.bytes().skip(1).all(|byte| byte == b'=')
            {
                self.blocks.table = None;
                return true;
            }
            return false;
        }
        if self.blocks.verbatim.is_some() {
            return false;
        }
        let mut bytes = line.bytes();
        if line.len() >= 4
            && matches!(bytes.next(), Some(b'|' | b'!' | b',' | b':'))
            && bytes.all(|byte| byte == b'=')
        {
            self.blocks.table = delimiter_key(line);
            return true;
        }
        false
    }

    /// Comment contents bypass preprocessing, unlike listing and passthrough
    /// contents, where include and conditional directives still apply.
    pub(super) fn preserves_comment_line(&mut self, line: &str) -> bool {
        if let Some(delimiter) = self.blocks.block_comment {
            if delimiter_key(line) == Some(delimiter) && line.bytes().all(|byte| byte == b'/') {
                self.blocks.block_comment = None;
            }
            return true;
        }
        if self.blocks.verbatim.is_none()
            && self.blocks.table.is_none()
            && line.len() >= 4
            && line.bytes().all(|byte| byte == b'/')
        {
            self.blocks.block_comment = delimiter_key(line);
            return true;
        }
        false
    }

    pub(super) fn block_context(&self) -> BlockContext {
        self.blocks
    }

    /// Includes share block boundaries with their caller, even when a fence is
    /// opened or closed in a different source file.
    pub(super) fn record_expansion(&mut self, text: &str) {
        for line in text.lines() {
            if !self.preserves_comment_line(line) && !self.at_table_delimiter(line) {
                self.at_verbatim_delimiter(line);
            }
            self.record(line);
        }
    }

    /// Whether `line` is an adjacent line comment the reader drops. Pure
    /// (`&self`): the drop path must not mutate state, so a run of adjacent
    /// comments is dropped together.
    pub(super) fn drops(&self, line: &str) -> bool {
        self.blocks.verbatim.is_none()
            && is_line_comment(line)
            && !super::tag::is_tag_directive_line(line)
            && self.prev_attaches
    }

    /// Record an emitted `line` so the next decision sees the correct context.
    pub(super) fn record(&mut self, line: &str) {
        self.prev_attaches = is_attaching_content(line, self.setext);
    }
}
