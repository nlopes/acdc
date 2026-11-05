//! Index-term discovery in inline content.

use std::fmt;

use acdc_parser::ast::{ContentRef, IndexTermRef, InlineKind, InlineRef};

use crate::InlineTextTransform;

/// Visit index occurrences in source order, including formatted text and link labels.
///
/// Offsets count UTF-8 bytes in decoded plain text with `\n` line breaks. Term
/// labels and relationship targets are not additional occurrences. The iterator
/// need not be cloneable, and discovery does not collect or register terms.
///
/// # Errors
///
/// Returns the first callback error without consuming later input nodes.
pub fn visit_index_inlines<'inline, E>(
    nodes: impl Iterator<Item = InlineRef<'inline>>,
    visit: &mut impl FnMut(IndexTermRef<'inline>, usize) -> Result<(), E>,
) -> Result<(), E> {
    let transform = InlineTextTransform::default()
        .line_break("\n")
        .decode_char_refs(true);
    let mut offset = TextOffset(0);
    for node in nodes {
        walk_node(node, &mut offset, &transform, visit)?;
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

fn walk_node<'inline, E>(
    node: InlineRef<'inline>,
    offset: &mut TextOffset,
    transform: &InlineTextTransform<'_, '_>,
    visit: &mut impl FnMut(IndexTermRef<'inline>, usize) -> Result<(), E>,
) -> Result<(), E> {
    if let InlineKind::IndexTerm(term) = node.kind() {
        visit(term, offset.0)?;
    } else if let Some(children) = index_children(node) {
        return children
            .inlines()
            .try_for_each(|node| walk_node(node, offset, transform, visit));
    }
    let _ = transform.write_inlines(offset, std::iter::once(node));
    Ok(())
}

fn index_children(node: InlineRef<'_>) -> Option<ContentRef<'_>> {
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
        InlineKind::Footnote(footnote) => return footnote.content(),
        InlineKind::Plain(_)
        | InlineKind::Raw(_)
        | InlineKind::Verbatim(_)
        | InlineKind::StandaloneCurvedApostrophe(_)
        | InlineKind::CharacterReference(_)
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
