//! Index-term discovery in inline content.

use std::fmt;

use acdc_parser::{
    IndexTerm, InlineMacro, InlineNode,
    ast::{ContentRef, IndexTermRef, InlineKind, InlineRef},
};

use crate::InlineTextTransform;

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
    let transform = InlineTextTransform::default().line_break("\n");
    let mut offset = TextOffset(0);
    for node in nodes {
        walk_node::<&[InlineNode<'_>], E>(node, &mut offset, &transform, visit)?;
    }
    Ok(())
}

/// Visit index occurrences from typed inlines in source order.
///
/// Offsets count UTF-8 bytes in plain text with `\n` line breaks. Descent and
/// label exclusions follow [`visit_index_terms`]. The iterator need not be
/// cloneable, and discovery does not collect or register terms.
///
/// # Errors
///
/// Returns the first callback error without consuming later input nodes.
pub fn visit_index_inlines<'inline, E>(
    nodes: impl Iterator<Item = InlineRef<'inline>>,
    visit: &mut impl FnMut(IndexTermRef<'inline>, usize) -> Result<(), E>,
) -> Result<(), E> {
    let transform = InlineTextTransform::default().line_break("\n");
    let mut offset = TextOffset(0);
    for node in nodes {
        walk_node::<ContentRef<'inline>, E>(node, &mut offset, &transform, visit)?;
    }
    Ok(())
}

struct TextOffset(usize);

impl fmt::Write for TextOffset {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.0 += text.len();
        Ok(())
    }
}

// Keep owner callbacks intact without recovering owners from typed handles.
trait IndexContent<'inline>: Copy {
    type Node: Copy;
    type Term;

    fn inline(node: Self::Node) -> InlineRef<'inline>;
    fn term(node: Self::Node) -> Option<Self::Term>;
    fn children(node: Self::Node) -> Option<Self>;
    fn visit_nodes<E>(self, visit: impl FnMut(Self::Node) -> Result<(), E>) -> Result<(), E>;
}

fn walk_node<'inline, C: IndexContent<'inline>, E>(
    node: C::Node,
    offset: &mut TextOffset,
    transform: &InlineTextTransform<'_, '_>,
    visit: &mut impl FnMut(C::Term, usize) -> Result<(), E>,
) -> Result<(), E> {
    if let Some(term) = C::term(node) {
        visit(term, offset.0)?;
    } else if let Some(children) = C::children(node) {
        return children.visit_nodes(|node| walk_node::<C, E>(node, offset, transform, visit));
    }
    let _ = transform.write_inlines(offset, std::iter::once(C::inline(node)));
    Ok(())
}

impl<'nodes, 'input: 'nodes> IndexContent<'nodes> for &'nodes [InlineNode<'input>] {
    type Node = &'nodes InlineNode<'input>;
    type Term = &'nodes IndexTerm<'input>;

    fn inline(node: Self::Node) -> InlineRef<'nodes> {
        InlineRef::from(node)
    }

    fn term(node: Self::Node) -> Option<Self::Term> {
        if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node {
            Some(term.as_ref())
        } else {
            None
        }
    }

    fn children(node: Self::Node) -> Option<Self> {
        Some(match node {
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
            InlineNode::Macro(InlineMacro::Footnote(footnote)) => &footnote.content,
            InlineNode::PlainText(_)
            | InlineNode::RawText(_)
            | InlineNode::VerbatimText(_)
            | InlineNode::StandaloneCurvedApostrophe(_)
            | InlineNode::Symbol(_)
            | InlineNode::LineBreak(_)
            | InlineNode::InlineAnchor(_)
            | InlineNode::Macro(_)
            | InlineNode::CalloutRef(_)
            | _ => return None,
        })
    }

    fn visit_nodes<E>(self, visit: impl FnMut(Self::Node) -> Result<(), E>) -> Result<(), E> {
        self.iter().try_for_each(visit)
    }
}

impl<'inline> IndexContent<'inline> for ContentRef<'inline> {
    type Node = InlineRef<'inline>;
    type Term = IndexTermRef<'inline>;

    fn inline(node: Self::Node) -> InlineRef<'inline> {
        node
    }

    fn term(node: Self::Node) -> Option<Self::Term> {
        if let InlineKind::IndexTerm(term) = node.kind() {
            Some(term)
        } else {
            None
        }
    }

    fn children(node: Self::Node) -> Option<Self> {
        Some(match node.kind() {
            InlineKind::Bold(text) => text.content(),
            InlineKind::Italic(text) => text.content(),
            InlineKind::Monospace(text) => text.content(),
            InlineKind::Highlight(text) => text.content(),
            InlineKind::Subscript(text) => text.content(),
            InlineKind::Superscript(text) => text.content(),
            InlineKind::CurvedQuotation(text) => text.content(),
            InlineKind::CurvedApostrophe(text) => text.content(),
            InlineKind::Url(link) => link.text(),
            InlineKind::Link(link) => link.text(),
            InlineKind::Mailto(link) => link.text(),
            InlineKind::CrossReference(link) => link.text(),
            InlineKind::Footnote(footnote) => footnote.content(),
            InlineKind::Plain(_)
            | InlineKind::Raw(_)
            | InlineKind::Verbatim(_)
            | InlineKind::StandaloneCurvedApostrophe(_)
            | InlineKind::Symbol(_)
            | InlineKind::LineBreak(_)
            | InlineKind::Anchor(_)
            | InlineKind::Callout(_)
            | InlineKind::Icon(_)
            | InlineKind::Image(_)
            | InlineKind::Keyboard(_)
            | InlineKind::Button(_)
            | InlineKind::Menu(_)
            | InlineKind::Autolink(_)
            | InlineKind::Pass(_)
            | InlineKind::Stem(_)
            | InlineKind::IndexTerm(_)
            | _ => return None,
        })
    }

    fn visit_nodes<E>(self, visit: impl FnMut(Self::Node) -> Result<(), E>) -> Result<(), E> {
        self.inlines().try_for_each(visit)
    }
}
