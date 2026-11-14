//! Comment context tracking for the preprocessor.
//!
//! Comment blocks bypass preprocessing on both paths. Verbatim and raw
//! blocks keep slash lines literal while still processing directives.
//! Asciidoctor mode extends this rule to explicitly styled verbatim paragraphs.
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
use crate::{
    CompatibilityMode, DocumentAttributes,
    grammar::{block_attribute_line, block_attribute_style, style_overrides_block_form},
};

/// A `//`-prefixed line comment, but not `///` (literal text) or `////` (a
/// block-comment delimiter). Un-trimmed on purpose: an indented `  //` is a
/// literal/indented line, not a comment.
fn is_line_comment(line: &str) -> bool {
    line.starts_with("//") && !line.starts_with("///")
}

/// A Setext underline: a uniform run of a single Setext underline character
/// (`=`, `-`, `~`, `^`, `+`). The char set comes from
/// [`crate::grammar::setext::char_to_level`].
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
    let rest = line.trim_start_matches(['=', '#']);
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum ParagraphContext {
    #[default]
    None,
    Pending,
    Active,
}

/// Block and paragraph boundaries shared across include buffers.
#[derive(Debug, Default, Clone)]
pub(super) struct BlockContext {
    /// The open verbatim/raw block delimiter, or `None` when outside one
    /// (so "inside a verbatim block" is `verbatim.is_some()`).
    verbatim: Option<(u8, usize)>,
    table: Option<(u8, usize)>,
    block_comment: Option<(u8, usize)>,
    compound: Vec<(u8, usize)>,
    paragraph: ParagraphContext,
    /// Whether a generated entry would continue ordinary paragraph text.
    paragraph_continuation: bool,
    pending_style: Option<String>,
    title_width: Option<usize>,
}

impl BlockContext {
    fn allows_attribute_entries(&self) -> bool {
        self.verbatim.is_none()
            && self.table.is_none()
            && self.paragraph != ParagraphContext::Active
    }

    pub(super) fn allows_scope_entry(&self) -> bool {
        self.allows_attribute_entries() && !self.paragraph_continuation
    }
}

/// Running block and line context for the comment rules in the module docs.
pub(super) struct CommentScanner {
    blocks: BlockContext,
    /// Whether the previous emitted line is content a comment would merge into.
    prev_attaches: bool,
    /// Whether Setext titles are active, so their underlines count as a heading
    /// boundary (see [`crate::grammar::setext::is_enabled`]).
    setext: bool,
    compatibility_mode: CompatibilityMode,
}

impl CommentScanner {
    /// Attribute declarations are inactive in raw blocks and styled verbatim
    /// paragraphs. Include and conditional directives still apply there.
    pub(super) fn allows_attribute_entries(&self) -> bool {
        self.blocks.allows_attribute_entries()
    }

    /// Start as if at a blank boundary so a leading comment is preserved.
    pub(super) fn new(
        setext: bool,
        blocks: BlockContext,
        compatibility_mode: CompatibilityMode,
    ) -> Self {
        Self {
            blocks,
            prev_attaches: false,
            setext,
            compatibility_mode,
        }
    }

    /// Outside table buffers, whether `line` is a verbatim/raw block delimiter.
    /// The caller must then push and [`record`](Self::record) it. A non-matching
    /// delimiter inside a block leaves it open but is still reported as a
    /// delimiter line.
    pub(super) fn at_verbatim_delimiter(&mut self, line: &str) -> bool {
        if self.blocks.table.is_some()
            || self.blocks.paragraph == ParagraphContext::Active
            || self.at_setext_heading(line)
        {
            return false;
        }
        let open = (b'-', 2);
        let delimiter = if line == "--"
            && (self.blocks.verbatim == Some(open)
                || self.blocks.verbatim.is_none()
                    && self.blocks.compound.last() != Some(&open)
                    && matches!(
                        self.blocks.pending_style.as_deref(),
                        Some("source" | "listing" | "literal" | "verse" | "pass")
                    )) {
            Some(open)
        } else {
            Preprocessor::is_verbatim_delimiter(line).and_then(delimiter_key)
        };
        match delimiter {
            Some(delimiter) if self.blocks.verbatim == Some(delimiter) => {
                self.blocks.verbatim = None;
                true
            }
            Some(delimiter) if self.blocks.verbatim.is_none() => {
                if delimiter == open {
                    self.blocks.compound.push(delimiter);
                }
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
        if self.blocks.verbatim.is_some() || self.blocks.paragraph == ParagraphContext::Active {
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
            if delimiter_key(line) == Some(delimiter)
                && line.bytes().all(|byte| byte == delimiter.0)
            {
                self.blocks.block_comment = None;
            }
            return true;
        }
        if self.blocks.verbatim.is_none()
            && self.blocks.table.is_none()
            && self.blocks.paragraph != ParagraphContext::Active
            && (line.len() >= 4 && line.bytes().all(|byte| byte == b'/')
                || line == "--"
                    && self.blocks.pending_style.as_deref() == Some("comment")
                    && self.blocks.compound.last() != Some(&(b'-', 2)))
        {
            self.blocks.block_comment = delimiter_key(line);
            if line == "--" {
                self.blocks.compound.push((b'-', 2));
            }
            return true;
        }
        false
    }

    pub(super) fn block_context(&self) -> BlockContext {
        self.blocks.clone()
    }

    pub(super) fn record_preprocessed(&mut self, text: &str, blocks: BlockContext) {
        self.blocks = blocks;
        if let Some(line) = text.lines().last() {
            self.prev_attaches = is_attaching_content(line, self.setext);
        }
    }

    /// Includes share block boundaries with their caller, even when a fence is
    /// opened or closed in a different source file.
    pub(super) fn record_expansion(&mut self, text: &str, attributes: &DocumentAttributes<'_>) {
        for line in text.lines() {
            if !self.preserves_comment_line(line) && !self.at_table_delimiter(line) {
                self.at_verbatim_delimiter(line);
            }
            self.record(line, attributes);
        }
    }

    /// Whether `line` is an adjacent line comment the reader drops. Pure
    /// (`&self`): the drop path must not mutate state, so a run of adjacent
    /// comments is dropped together.
    pub(super) fn drops(&self, line: &str) -> bool {
        self.blocks.verbatim.is_none()
            && self.blocks.paragraph != ParagraphContext::Active
            && is_line_comment(line)
            && !super::tag::is_tag_directive_line(line)
            && (self.prev_attaches || self.blocks.paragraph == ParagraphContext::Pending)
    }

    /// Record an emitted `line` so the next decision sees the correct context.
    pub(super) fn record(&mut self, line: &str, attributes: &DocumentAttributes<'_>) {
        let continued_paragraph = self.blocks.paragraph_continuation;
        self.blocks.paragraph_continuation = !line.is_empty() && line != "+";
        let heading = self.at_setext_heading(line);
        let first_content = self.blocks.paragraph == ParagraphContext::Pending
            || self.blocks.paragraph == ParagraphContext::None && !self.prev_attaches;
        self.blocks.title_width = (self.setext
            && first_content
            && crate::grammar::setext::is_title(line)
            && !line.starts_with(['[', ':', '.', '/', '=', '#'])
            && !crate::grammar::helpers::title_looks_like_description_list(line))
        .then(|| line.trim().chars().count());
        self.prev_attaches = is_attaching_content(line, self.setext);
        if self.blocks.verbatim.is_some()
            || self.blocks.table.is_some()
            || self.blocks.block_comment.is_some()
        {
            self.blocks.paragraph = ParagraphContext::None;
            self.blocks.paragraph_continuation = false;
            self.blocks.pending_style = None;
            return;
        }

        if heading {
            self.blocks.pending_style = None;
            self.blocks.paragraph = ParagraphContext::None;
            self.blocks.title_width = None;
            self.blocks.paragraph_continuation = false;
            return;
        }
        let compound = delimiter_key(line).filter(|&(byte, length)| {
            (matches!(byte, b'=' | b'*' | b'_') && length >= 4 || byte == b'-' && length == 2)
                && line.bytes().all(|character| character == byte)
        });
        if compound.is_some() && compound.as_ref() == self.blocks.compound.last() {
            self.blocks.compound.pop();
            self.blocks.pending_style = None;
            self.blocks.paragraph = ParagraphContext::None;
            self.blocks.paragraph_continuation = false;
        } else if self.blocks.paragraph == ParagraphContext::Active {
            // Only a real paragraph or enclosing-block boundary ends code;
            // apparent metadata and delimiters within it remain literal.
            if line.is_empty() || line == "+" {
                self.blocks.paragraph = ParagraphContext::None;
            }
        } else if let Some(compound) = compound {
            self.blocks.compound.push(compound);
            self.blocks.pending_style = None;
            self.blocks.paragraph = ParagraphContext::None;
            self.blocks.paragraph_continuation = false;
        } else if Preprocessor::is_verbatim_delimiter(line).is_some() || is_title_line(line, false)
        {
            self.blocks.pending_style = None;
            self.blocks.paragraph = ParagraphContext::None;
            self.blocks.paragraph_continuation =
                continued_paragraph && Preprocessor::is_verbatim_delimiter(line).is_none();
        } else if let Some(list) = block_attribute_line(line, attributes, self.compatibility_mode) {
            self.blocks.paragraph_continuation = continued_paragraph;
            if let Some(style) = block_attribute_style(list, attributes, self.compatibility_mode) {
                self.blocks.paragraph =
                    if style_overrides_block_form(self.compatibility_mode, Some(&style)) {
                        ParagraphContext::Pending
                    } else {
                        ParagraphContext::None
                    };
                self.blocks.pending_style = Some(style);
            }
        } else if !line.is_empty()
            && !is_line_comment(line)
            && !super::attribute::is_declaration(line)
            && !line
                .strip_prefix('.')
                .is_some_and(|title| !title.is_empty() && !title.starts_with([' ', '\t', '.']))
        {
            if self.blocks.paragraph == ParagraphContext::Pending {
                self.blocks.paragraph = ParagraphContext::Active;
            }
            self.blocks.pending_style = None;
            if !continued_paragraph
                && line.ends_with(']')
                && line
                    .split_once("::")
                    .is_some_and(|(name, _)| matches!(name, "image" | "audio" | "video" | "toc"))
            {
                self.blocks.paragraph_continuation = false;
            }
        } else {
            self.blocks.paragraph_continuation &= continued_paragraph;
        }
    }

    // A two-line heading precedes paragraph dispatch. Its underline must not
    // open a listing/example block or leave a preceding style active.
    fn at_setext_heading(&self, line: &str) -> bool {
        self.setext
            && self.blocks.verbatim.is_none()
            && self.blocks.table.is_none()
            && self.blocks.compound.is_empty()
            && (self.blocks.paragraph != ParagraphContext::Active || !line.starts_with('='))
            && is_setext_underline(line)
            && self
                .blocks
                .title_width
                .is_some_and(|width| crate::grammar::setext::width_ok(width, line.chars().count()))
    }
}
