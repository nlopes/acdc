//! Walking the block tree.
//!
//! Both halves of the pass — finding the elements to list and replacing the
//! macro calls — visit the same shape of tree, so the recursion lives here
//! once. It descends into everything that can hold blocks, including `AsciiDoc`
//! table cells, which is what asciidoctor-lists' `traverse_documents: true`
//! reaches.

use acdc_parser::{Block, BlockMetadata, DelimitedBlockType, Location, Table, Title};

/// Run `visit` over `blocks` and everything nested inside them, in document
/// order.
///
/// `visit` decides whether the walk descends into the block it was given, so a
/// caller that replaces a block can stop the walk from also visiting the parts
/// it just rewrote.
pub(crate) fn walk_blocks<'a, F>(blocks: &mut Vec<Block<'a>>, visit: &mut F)
where
    F: FnMut(&mut Block<'a>) -> Descend,
{
    for block in blocks.iter_mut() {
        if visit(block) == Descend::Yes {
            descend(block, visit);
        }
    }
}

/// Whether the walk should continue into a block's children.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Descend {
    /// Visit the block's children.
    Yes,
    /// Leave the block's children alone.
    No,
}

/// Visit everything nested inside one block.
pub(crate) fn descend<'a, F>(block: &mut Block<'a>, visit: &mut F)
where
    F: FnMut(&mut Block<'a>) -> Descend,
{
    match block {
        Block::Section(section) => walk_blocks(&mut section.content, visit),
        Block::Admonition(admonition) => walk_blocks(&mut admonition.blocks, visit),
        Block::UnorderedList(list) => {
            for item in &mut list.items {
                walk_blocks(&mut item.blocks, visit);
            }
        }
        Block::OrderedList(list) => {
            for item in &mut list.items {
                walk_blocks(&mut item.blocks, visit);
            }
        }
        Block::CalloutList(list) => {
            for item in &mut list.items {
                walk_blocks(&mut item.blocks, visit);
            }
        }
        Block::DescriptionList(list) => {
            for item in &mut list.items {
                walk_blocks(&mut item.description, visit);
            }
        }
        Block::DelimitedBlock(delimited) => match &mut delimited.inner {
            DelimitedBlockType::DelimitedExample(blocks)
            | DelimitedBlockType::DelimitedOpen(blocks)
            | DelimitedBlockType::DelimitedSidebar(blocks)
            | DelimitedBlockType::DelimitedQuote(blocks) => walk_blocks(blocks, visit),
            DelimitedBlockType::DelimitedTable(table) => walk_table(table, visit),
            // Verbatim content holds inline nodes, not blocks.
            DelimitedBlockType::DelimitedComment(_)
            | DelimitedBlockType::DelimitedListing(_)
            | DelimitedBlockType::DelimitedLiteral(_)
            | DelimitedBlockType::DelimitedPass(_)
            | DelimitedBlockType::DelimitedVerse(_)
            | DelimitedBlockType::DelimitedStem(_)
            | _ => {}
        },
        // The remaining kinds hold no blocks of their own.
        Block::TableOfContents(_)
        | Block::DiscreteHeader(_)
        | Block::DocumentAttribute(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::Paragraph(_)
        | Block::Image(_)
        | Block::Audio(_)
        | Block::Video(_)
        | Block::Comment(_)
        | _ => {}
    }
}

/// Visit the blocks inside a table's cells.
fn walk_table<'a, F>(table: &mut Table<'a>, visit: &mut F)
where
    F: FnMut(&mut Block<'a>) -> Descend,
{
    let rows = table
        .header
        .iter_mut()
        .chain(table.rows.iter_mut())
        .chain(table.footer.iter_mut());
    for row in rows {
        for column in &mut row.columns {
            walk_blocks(&mut column.content, visit);
        }
    }
}

/// A block's metadata, for the blocks that carry any.
///
/// `acdc_parser::Block` has this accessor, but not publicly, and the pass
/// needs the mutable form to give an element an id.
pub(crate) fn metadata_mut<'m, 'a>(block: &'m mut Block<'a>) -> Option<&'m mut BlockMetadata<'a>> {
    match block {
        Block::Section(b) => Some(&mut b.metadata),
        Block::DelimitedBlock(b) => Some(&mut b.metadata),
        Block::Admonition(b) => Some(&mut b.metadata),
        Block::DiscreteHeader(b) => Some(&mut b.metadata),
        Block::PageBreak(b) => Some(&mut b.metadata),
        Block::Paragraph(b) => Some(&mut b.metadata),
        Block::Image(b) => Some(&mut b.metadata),
        Block::Audio(b) => Some(&mut b.metadata),
        Block::Video(b) => Some(&mut b.metadata),
        Block::UnorderedList(b) => Some(&mut b.metadata),
        Block::OrderedList(b) => Some(&mut b.metadata),
        Block::CalloutList(b) => Some(&mut b.metadata),
        Block::DescriptionList(b) => Some(&mut b.metadata),
        Block::TableOfContents(b) => Some(&mut b.metadata),
        Block::DocumentAttribute(_) | Block::ThematicBreak(_) | Block::Comment(_) | _ => None,
    }
}

/// A block's title, if it has a non-empty one.
pub(crate) fn title_of<'m, 'a>(block: &'m Block<'a>) -> Option<&'m Title<'a>> {
    let title = match block {
        Block::Section(b) => &b.title,
        Block::DelimitedBlock(b) => &b.title,
        Block::Admonition(b) => &b.title,
        Block::DiscreteHeader(b) => &b.title,
        Block::PageBreak(b) => &b.title,
        Block::Paragraph(b) => &b.title,
        Block::Image(b) => &b.title,
        Block::Audio(b) => &b.title,
        Block::Video(b) => &b.title,
        Block::UnorderedList(b) => &b.title,
        Block::OrderedList(b) => &b.title,
        Block::CalloutList(b) => &b.title,
        Block::DescriptionList(b) => &b.title,
        Block::ThematicBreak(b) => &b.title,
        Block::TableOfContents(_) | Block::DocumentAttribute(_) | Block::Comment(_) | _ => {
            return None;
        }
    };
    (!title.is_empty()).then_some(title)
}

/// A block's own source location, for the catalog entry of an id the pass
/// assigns.
///
/// `acdc_parser` has a `Locateable` trait for this, but does not export it.
pub(crate) fn location_of(block: &Block<'_>) -> Location {
    match block {
        Block::Section(b) => b.location.clone(),
        Block::DelimitedBlock(b) => b.location.clone(),
        Block::Admonition(b) => b.location.clone(),
        Block::DiscreteHeader(b) => b.location.clone(),
        Block::PageBreak(b) => b.location.clone(),
        Block::ThematicBreak(b) => b.location.clone(),
        Block::Paragraph(b) => b.location.clone(),
        Block::Image(b) => b.location.clone(),
        Block::Audio(b) => b.location.clone(),
        Block::Video(b) => b.location.clone(),
        Block::UnorderedList(b) => b.location.clone(),
        Block::OrderedList(b) => b.location.clone(),
        Block::CalloutList(b) => b.location.clone(),
        Block::DescriptionList(b) => b.location.clone(),
        Block::TableOfContents(b) => b.location.clone(),
        Block::DocumentAttribute(b) => b.location.clone(),
        Block::Comment(_) | _ => Location::default(),
    }
}
