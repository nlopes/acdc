use std::{iter::FusedIterator, mem, slice};

use crate::{Block, CalloutListItem, DescriptionListItem, Header, ListItem, TableColumn, TableRow};

use super::{
    BlockKind, BlockRef, CalloutListItemRef, DescriptionListItemRef, DocumentRef, HeaderRef,
    ListItemRef, TableCellRef, TableRowRef,
};

/// A document, header, block, list item, table row, or table cell.
/// Inline content, scalar metadata, and derived catalogs are outside this traversal.
///
/// ```
/// use acdc_parser::{Options, parse, ast::{BlockKind, StructuralRef}};
///
/// let parsed = parse("* Item\n+\nAttached.\n", &Options::default())?;
/// let paragraph = parsed.document_ref().descendants().find_map(|node| {
///     if let StructuralRef::Block(block) = node {
///         if let BlockKind::Paragraph(paragraph) = block.kind() {
///             return Some(paragraph);
///         }
///     }
///     None
/// });
/// assert!(paragraph.is_some());
/// # Ok::<(), acdc_parser::Error>(())
/// ```
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum StructuralRef<'doc> {
    Document(DocumentRef<'doc>),
    Header(HeaderRef<'doc>),
    Block(BlockRef<'doc>),
    ListItem(ListItemRef<'doc>),
    DescriptionListItem(DescriptionListItemRef<'doc>),
    CalloutListItem(CalloutListItemRef<'doc>),
    TableRow(TableRowRef<'doc>),
    TableCell(TableCellRef<'doc>),
}

impl<'doc> StructuralRef<'doc> {
    /// Direct children without allocation. A document's header precedes its blocks;
    /// lists yield items, and tables yield header, body, then footer rows. Each row
    /// yields cells in stored order, with spanning cells occurring once.
    /// Other containers yield their retained blocks in order.
    pub fn children(self) -> impl Iterator<Item = Self> + FusedIterator + Send + Sync + 'doc {
        self.child_iter()
    }

    /// Depth-first pre-order traversal, excluding this node.
    ///
    /// Children follow [`Self::children`]. Stop with ordinary iterator operations
    /// such as `find`, `take`, or `try_for_each`; unrequested descendants are not visited.
    /// Flat walks allocate no memory. Nested walks use a growable stack of pending
    /// sibling iterators, whose space grows with nesting rather than node count.
    /// Traversal does not recurse on the call stack.
    pub fn descendants(self) -> impl Iterator<Item = Self> + FusedIterator + Send + Sync + 'doc {
        Descendants {
            current: self.child_iter(),
            ancestors: Vec::new(),
            pending: None,
        }
    }

    pub(super) fn child_iter(self) -> Children<'doc> {
        match self {
            Self::Document(document) => Children::Document {
                header: document.node.header.as_ref(),
                blocks: document.node.blocks.iter(),
            },
            Self::Block(block) => match block.kind() {
                BlockKind::Section(section) => Children::Blocks(section.node.content.iter()),
                BlockKind::Admonition(admonition) => {
                    Children::Blocks(admonition.node.blocks.iter())
                }
                BlockKind::Example(block) => Children::Blocks(block.body.iter()),
                BlockKind::Open(block) => Children::Blocks(block.body.iter()),
                BlockKind::Sidebar(block) => Children::Blocks(block.body.iter()),
                BlockKind::Quote(block) => Children::Blocks(block.body.iter()),
                BlockKind::UnorderedList(list) => Children::Items(list.node.items.iter()),
                BlockKind::OrderedList(list) => Children::Items(list.node.items.iter()),
                BlockKind::DescriptionList(list) => Children::Descriptions(list.node.items.iter()),
                BlockKind::CalloutList(list) => Children::Callouts(list.node.items.iter()),
                BlockKind::Table(table) => Children::Rows {
                    header: table.table.header.as_ref(),
                    rows: table.table.rows.iter(),
                    footer: table.table.footer.as_ref(),
                },
                BlockKind::TableOfContents(_)
                | BlockKind::DiscreteHeader(_)
                | BlockKind::DocumentAttribute(_)
                | BlockKind::ThematicBreak(_)
                | BlockKind::PageBreak(_)
                | BlockKind::Paragraph(_)
                | BlockKind::Image(_)
                | BlockKind::Audio(_)
                | BlockKind::Video(_)
                | BlockKind::Comment(_)
                | BlockKind::DelimitedComment(_)
                | BlockKind::Listing(_)
                | BlockKind::Literal(_)
                | BlockKind::Passthrough(_)
                | BlockKind::Verse(_)
                | BlockKind::Stem(_) => Children::Empty,
            },
            Self::ListItem(item) => Children::Blocks(item.node.blocks.iter()),
            Self::DescriptionListItem(item) => Children::Blocks(item.node.description.iter()),
            Self::CalloutListItem(item) => Children::Blocks(item.node.blocks.iter()),
            Self::TableRow(row) => Children::Cells(row.row.columns.iter()),
            Self::TableCell(cell) => Children::Blocks(cell.cell.content.iter()),
            Self::Header(_) => Children::Empty,
        }
    }
}

macro_rules! structural_reads {
    ($($variant:ident($read:ident)),+ $(,)?) => { $(
        impl<'doc> From<$read<'doc>> for StructuralRef<'doc> {
            fn from(node: $read<'doc>) -> Self {
                Self::$variant(node)
            }
        }
    )+ };
}

structural_reads!(
    Document(DocumentRef),
    Header(HeaderRef),
    Block(BlockRef),
    ListItem(ListItemRef),
    DescriptionListItem(DescriptionListItemRef),
    CalloutListItem(CalloutListItemRef),
    TableRow(TableRowRef),
    TableCell(TableCellRef),
);

macro_rules! traversal_roots {
    ($($read:ident),+) => { $(
        impl<'doc> $read<'doc> {
            /// Direct structural children in [`StructuralRef::children`] order.
            pub fn children(self) -> impl Iterator<Item = StructuralRef<'doc>> + FusedIterator + Send + Sync + 'doc {
                StructuralRef::from(self).children()
            }

            /// Descendants in depth-first pre-order, excluding this node.
            /// See [`StructuralRef::descendants`] for order and stack behavior.
            pub fn descendants(self) -> impl Iterator<Item = StructuralRef<'doc>> + FusedIterator + Send + Sync + 'doc {
                StructuralRef::from(self).descendants()
            }
        }
    )+ };
}

traversal_roots!(DocumentRef, BlockRef);

#[derive(Debug)]
pub(super) enum Children<'doc> {
    Empty,
    Document {
        header: Option<&'doc Header<'doc>>,
        blocks: slice::Iter<'doc, Block<'doc>>,
    },
    Blocks(slice::Iter<'doc, Block<'doc>>),
    Items(slice::Iter<'doc, ListItem<'doc>>),
    Descriptions(slice::Iter<'doc, DescriptionListItem<'doc>>),
    Callouts(slice::Iter<'doc, CalloutListItem<'doc>>),
    Rows {
        header: Option<&'doc TableRow<'doc>>,
        rows: slice::Iter<'doc, TableRow<'doc>>,
        footer: Option<&'doc TableRow<'doc>>,
    },
    Cells(slice::Iter<'doc, TableColumn<'doc>>),
}

impl Children<'_> {
    fn is_empty(&self) -> bool {
        match self {
            Self::Empty => true,
            Self::Document { header, blocks } => header.is_none() && blocks.len() == 0,
            Self::Blocks(blocks) => blocks.len() == 0,
            Self::Items(items) => items.len() == 0,
            Self::Descriptions(items) => items.len() == 0,
            Self::Callouts(items) => items.len() == 0,
            Self::Rows {
                header,
                rows,
                footer,
            } => header.is_none() && rows.len() == 0 && footer.is_none(),
            Self::Cells(cells) => cells.len() == 0,
        }
    }
}

impl<'doc> Iterator for Children<'doc> {
    type Item = StructuralRef<'doc>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Empty => None,
            Self::Document { header, blocks } => header
                .take()
                .map(|header| HeaderRef::from(header).into())
                .or_else(|| blocks.next().map(|block| BlockRef::from(block).into())),
            Self::Blocks(blocks) => blocks.next().map(|block| BlockRef::from(block).into()),
            Self::Items(items) => items.next().map(|item| ListItemRef::from(item).into()),
            Self::Descriptions(items) => items
                .next()
                .map(|item| DescriptionListItemRef::from(item).into()),
            Self::Callouts(items) => items
                .next()
                .map(|item| CalloutListItemRef::from(item).into()),
            Self::Rows {
                header,
                rows,
                footer,
            } => header
                .take()
                .or_else(|| rows.next())
                .or_else(|| footer.take())
                .map(|row| TableRowRef::from(row).into()),
            Self::Cells(cells) => cells.next().map(|cell| TableCellRef::from(cell).into()),
        }
    }
}

impl FusedIterator for Children<'_> {}

struct Descendants<'doc> {
    current: Children<'doc>,
    ancestors: Vec<Children<'doc>>,
    pending: Option<StructuralRef<'doc>>,
}

impl<'doc> Iterator for Descendants<'doc> {
    type Item = StructuralRef<'doc>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(node) = self.pending.take() {
            let children = node.child_iter();
            if !children.is_empty() {
                let parent = mem::replace(&mut self.current, children);
                if !parent.is_empty() {
                    self.ancestors.push(parent);
                }
            }
        }
        loop {
            if let Some(node) = self.current.next() {
                self.pending = Some(node);
                return Some(node);
            }
            self.current = self.ancestors.pop()?;
        }
    }
}

impl FusedIterator for Descendants<'_> {}
