use std::fmt::{self, Write};

use crate::{InlineMacro, InlineNode};

/// Write plain-text extracted from `inlines` into `w`, recursing through
/// inline formatting nodes (bold, italic, monospace, etc.).
///
/// Streams directly into the writer — no per-node `String` allocations and
/// no intermediate concatenation. Callers that want a `String` should use
/// [`inlines_to_string`], which is a thin wrapper.
///
/// The writer is `fmt::Write` (not `io::Write`) deliberately: the parser's
/// inline content is already valid UTF-8 (every field is `&'a str`), and a
/// `fmt::Write` buffer — most commonly a `String` — avoids the `from_utf8`
/// validation pass an `io::Write` round-trip would require under the
/// workspace-level `unsafe_code = "forbid"` policy.
///
/// # Errors
///
/// Returns any error produced by the underlying writer.
pub(crate) fn write_inlines<W: Write + ?Sized>(
    w: &mut W,
    inlines: &[InlineNode<'_>],
) -> fmt::Result {
    for node in inlines {
        write_inline_node(w, node)?;
    }
    Ok(())
}

fn write_inline_node<W: Write + ?Sized>(w: &mut W, node: &InlineNode<'_>) -> fmt::Result {
    match node {
        InlineNode::PlainText(text) => w.write_str(text.content),
        InlineNode::RawText(text) => w.write_str(text.content),
        InlineNode::VerbatimText(text) => w.write_str(text.content),
        InlineNode::BoldText(bold) => write_inlines(w, &bold.content),
        InlineNode::ItalicText(italic) => write_inlines(w, &italic.content),
        InlineNode::MonospaceText(mono) => write_inlines(w, &mono.content),
        InlineNode::HighlightText(highlight) => write_inlines(w, &highlight.content),
        InlineNode::SubscriptText(sub) => write_inlines(w, &sub.content),
        InlineNode::SuperscriptText(sup) => write_inlines(w, &sup.content),
        InlineNode::CurvedQuotationText(quote) => write_inlines(w, &quote.content),
        InlineNode::CurvedApostropheText(apos) => write_inlines(w, &apos.content),
        InlineNode::StandaloneCurvedApostrophe(_) => w.write_char('\''),
        InlineNode::LineBreak(_) => w.write_char(' '),
        InlineNode::InlineAnchor(_) => Ok(()),
        InlineNode::Macro(macro_node) => write_inline_macro(w, macro_node),
        InlineNode::CalloutRef(callout) => write!(w, "<{}>", callout.number),
    }
}

fn write_inline_macro<W: Write + ?Sized>(w: &mut W, m: &InlineMacro<'_>) -> fmt::Result {
    match m {
        InlineMacro::Link(link) => match link.text.as_ref() {
            Some(text) => write!(w, "{text}"),
            None => write!(w, "{}", link.target),
        },
        InlineMacro::Url(url) => {
            if url.text.is_empty() {
                write!(w, "{}", url.target)
            } else {
                write_inlines(w, &url.text)
            }
        }
        InlineMacro::Mailto(mailto) => {
            if mailto.text.is_empty() {
                write!(w, "{}", mailto.target)
            } else {
                write_inlines(w, &mailto.text)
            }
        }
        InlineMacro::Autolink(autolink) => write!(w, "{}", autolink.url),
        InlineMacro::CrossReference(xref) => {
            if xref.text.is_empty() {
                write!(w, "{}", xref.target)
            } else {
                write_inlines(w, &xref.text)
            }
        }
        InlineMacro::IndexTerm(index_term) if index_term.is_visible() => {
            w.write_str(index_term.term())
        }
        InlineMacro::Image(_)
        | InlineMacro::Footnote(_)
        | InlineMacro::Button(_)
        | InlineMacro::Pass(_)
        | InlineMacro::Keyboard(_)
        | InlineMacro::Menu(_)
        | InlineMacro::Stem(_)
        | InlineMacro::Icon(_)
        | InlineMacro::IndexTerm(_) => Ok(()),
    }
}

/// Extract plain text from `inlines` as a `String`, recursively handling inline
/// formatting nodes.
///
/// Thin wrapper for callers that don't already have a writer. Allocates exactly one
/// `String` for the whole subtree.
#[must_use]
pub fn inlines_to_string(inlines: &[InlineNode<'_>]) -> String {
    let mut s = String::new();
    // Writing into a `String` is infallible.
    let _ = write_inlines(&mut s, inlines);
    s
}
