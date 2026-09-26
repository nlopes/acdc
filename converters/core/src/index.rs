//! Index-term discovery in inline content.

use std::fmt;

use crate::InlineTextTransform;
use acdc_parser::{IndexTerm, InlineMacro, InlineNode};

/// Visit index occurrences in source order, including terms in formatted text.
/// Each callback receives the byte offset in plain text with `\n` line breaks.
/// Term labels and relationship targets are not additional occurrences.
///
/// # Errors
/// Returns the first error from `visit`.
pub fn visit_index_terms<E>(
    nodes: &[InlineNode<'_>],
    visit: &mut impl FnMut(&IndexTerm<'_>, usize) -> Result<(), E>,
) -> Result<(), E> {
    walk_terms(nodes, &mut TextOffset(0), visit)
}

struct TextOffset(usize);

impl fmt::Write for TextOffset {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.0 += text.len();
        Ok(())
    }
}

fn walk_terms<E>(
    nodes: &[InlineNode<'_>],
    offset: &mut TextOffset,
    visit: &mut impl FnMut(&IndexTerm<'_>, usize) -> Result<(), E>,
) -> Result<(), E> {
    let transform = InlineTextTransform::default().line_break("\n");
    for node in nodes {
        let children = match node {
            InlineNode::BoldText(text) => &text.content,
            InlineNode::ItalicText(text) => &text.content,
            InlineNode::MonospaceText(text) => &text.content,
            InlineNode::HighlightText(text) => &text.content,
            InlineNode::SubscriptText(text) => &text.content,
            InlineNode::SuperscriptText(text) => &text.content,
            InlineNode::CurvedQuotationText(text) => &text.content,
            InlineNode::CurvedApostropheText(text) => &text.content,
            InlineNode::Macro(InlineMacro::Url(link)) => &link.text,
            InlineNode::Macro(InlineMacro::Link(link)) => &link.text,
            InlineNode::Macro(InlineMacro::Mailto(link)) => &link.text,
            InlineNode::Macro(InlineMacro::CrossReference(link)) => &link.text,
            InlineNode::Macro(InlineMacro::Footnote(note)) => &note.content,
            InlineNode::Macro(InlineMacro::IndexTerm(term)) => {
                visit(term, offset.0)?;
                let _ = transform.write(offset, std::slice::from_ref(node));
                continue;
            }
            InlineNode::PlainText(_)
            | InlineNode::RawText(_)
            | InlineNode::VerbatimText(_)
            | InlineNode::StandaloneCurvedApostrophe(_)
            | InlineNode::LineBreak(_)
            | InlineNode::InlineAnchor(_)
            | InlineNode::Macro(_)
            | InlineNode::CalloutRef(_)
            | _ => {
                let _ = transform.write(offset, std::slice::from_ref(node));
                continue;
            }
        };
        walk_terms(children, offset, visit)?;
    }
    Ok(())
}
