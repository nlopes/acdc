//! Index terms registered while automatic heading IDs are prepared.

use acdc_converters_core::{TraversalContext, visitor::Visitor};
use acdc_parser::{
    BlockMetadata, DiscreteHeader, Document, IndexTerm, InlineMacro, InlineNode, Section,
};

pub(crate) fn visit_terms<'doc, E>(
    document: &'doc Document<'doc>,
    visit: impl FnMut(&mut TraversalContext<'doc>, &IndexTerm<'_>) -> Result<(), E>,
) -> Result<(), E> {
    let mut traversal = TraversalContext::new(&document.attributes);
    traversal.visit_blocks(&mut HeadingTerms(visit), &document.blocks)
}

struct HeadingTerms<F>(F);

impl<'doc, E, F> Visitor<'doc> for HeadingTerms<F>
where
    F: FnMut(&mut TraversalContext<'doc>, &IndexTerm<'_>) -> Result<(), E>,
{
    type Error = E;

    fn visit_section(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        section: &'doc Section<'doc>,
    ) -> Result<(), E> {
        if has_automatic_id(traversal, &section.metadata) {
            visit_inline_terms(&section.title, &mut |term| (self.0)(traversal, term))?;
        }
        traversal.visit_blocks(self, &section.content)
    }

    fn visit_discrete_header(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        header: &DiscreteHeader<'_>,
    ) -> Result<(), E> {
        if has_automatic_id(traversal, &header.metadata) {
            visit_inline_terms(&header.title, &mut |term| (self.0)(traversal, term))?;
        }
        Ok(())
    }
}

fn has_automatic_id(traversal: &TraversalContext<'_>, metadata: &BlockMetadata<'_>) -> bool {
    traversal.contains_key("sectids") && metadata.id.is_none() && metadata.anchors.is_empty()
}

fn visit_inline_terms<E>(
    nodes: &[InlineNode<'_>],
    visit: &mut impl FnMut(&IndexTerm<'_>) -> Result<(), E>,
) -> Result<(), E> {
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
                visit(term)?;
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
            | _ => continue,
        };
        visit_inline_terms(children, visit)?;
    }
    Ok(())
}
