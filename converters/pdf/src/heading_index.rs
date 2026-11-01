//! Index terms registered while automatic heading IDs are prepared.

use acdc_converters_core::{TraversalContext, index::visit_index_inlines, visitor::BlockVisitor};
use acdc_parser::{
    ColumnStyle,
    ast::{BlockKind, BlockRef, DocumentRef, IndexTermRef, StructuralRef, TableRef},
};

pub(crate) fn visit_terms<'doc, E>(
    document: DocumentRef<'doc>,
    visit: impl FnMut(&mut TraversalContext<'doc>, IndexTermRef<'_>) -> Result<(), E>,
) -> Result<(), E> {
    let mut traversal = TraversalContext::new(document.attributes());
    traversal.visit_blocks(&mut HeadingTerms(visit), document.blocks())
}

struct HeadingTerms<F>(F);

impl<'doc, E, F> BlockVisitor<'doc> for HeadingTerms<F>
where
    F: FnMut(&mut TraversalContext<'doc>, IndexTermRef<'_>) -> Result<(), E>,
{
    type Error = E;

    fn visit_block(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        block: BlockRef<'doc>,
    ) -> Result<(), E> {
        if matches!(
            block.kind(),
            BlockKind::Section(_) | BlockKind::DiscreteHeader(_)
        ) && traversal.contains_key("sectids")
            && block.primary_anchor().is_none()
            && let Some(title) = block.title()
        {
            visit_index_inlines(title.inlines(), &mut |term, _| (self.0)(traversal, term))?;
        }
        if let BlockKind::Table(table) = block.kind() {
            visit_table(self, traversal, table)
        } else {
            visit_children(self, traversal, block.into())
        }
    }
}

fn visit_children<'doc, V: BlockVisitor<'doc>>(
    visitor: &mut V,
    traversal: &mut TraversalContext<'doc>,
    parent: StructuralRef<'doc>,
) -> Result<(), V::Error> {
    for child in parent.children() {
        if let StructuralRef::Block(block) = child {
            traversal.visit_block(visitor, block)?;
        } else {
            visit_children(visitor, traversal, child)?;
        }
    }
    Ok(())
}

fn visit_table<'doc, V: BlockVisitor<'doc>>(
    visitor: &mut V,
    traversal: &mut TraversalContext<'doc>,
    table: TableRef<'doc>,
) -> Result<(), V::Error> {
    for row in table
        .header()
        .into_iter()
        .chain(table.rows())
        .chain(table.footer())
    {
        let mut columns = table.columns();
        for cell in row.cells() {
            let inherited = columns.next().map_or(
                ColumnStyle::Default,
                acdc_parser::ast::ColumnFormatRef::style,
            );
            if cell.style().unwrap_or(inherited) == ColumnStyle::AsciiDoc {
                traversal.with_table_cell(cell, |context| {
                    context.visit_blocks(visitor, cell.blocks())
                })?;
            } else {
                traversal.visit_blocks(visitor, cell.blocks())?;
            }
        }
    }
    Ok(())
}
