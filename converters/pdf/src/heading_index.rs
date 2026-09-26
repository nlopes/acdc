//! Index terms registered while automatic heading IDs are prepared.

use acdc_converters_core::{TraversalContext, index::visit_index_terms, visitor::Visitor};
use acdc_parser::{BlockMetadata, DiscreteHeader, Document, IndexTerm, Section};

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
            visit_index_terms(&section.title, &mut |term, _| (self.0)(traversal, term))?;
        }
        traversal.visit_blocks(self, &section.content)
    }

    fn visit_discrete_header(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        header: &DiscreteHeader<'_>,
    ) -> Result<(), E> {
        if has_automatic_id(traversal, &header.metadata) {
            visit_index_terms(&header.title, &mut |term, _| (self.0)(traversal, term))?;
        }
        Ok(())
    }
}

fn has_automatic_id(traversal: &TraversalContext<'_>, metadata: &BlockMetadata<'_>) -> bool {
    traversal.contains_key("sectids") && metadata.id.is_none() && metadata.anchors.is_empty()
}
