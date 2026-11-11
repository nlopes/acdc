use crate::{Block, ColumnStyle, DelimitedBlockType, Table};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Children {
    Body,
    Item(usize),
    Cell {
        part: TablePart,
        row: usize,
        column: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TablePart {
    Head,
    Body,
    Foot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BlockPath {
    root: usize,
    nested: Vec<(Children, usize)>,
}

impl BlockPath {
    pub(super) fn container_mut<'doc, 'a>(
        &self,
        blocks: &'doc mut Vec<Block<'a>>,
    ) -> Option<(&'doc mut Vec<Block<'a>>, usize)> {
        let Some((&(children, index), parents)) = self.nested.split_last() else {
            return Some((blocks, self.root));
        };
        let mut parent = blocks.get_mut(self.root)?;
        for &(children, index) in parents {
            parent = children.get_mut(parent)?.get_mut(index)?;
        }
        Some((children.get_mut(parent)?, index))
    }

    pub(super) fn is_within(&self, parent: &Self) -> bool {
        self.root == parent.root && self.nested.starts_with(&parent.nested)
    }

    pub(super) fn shift_siblings(&mut self, at: &Self, insert: bool) {
        let index = if let Some((&(children, position), parents)) = at.nested.split_last() {
            if self.root != at.root || !self.nested.starts_with(parents) {
                return;
            }
            let Some((own_children, own_position)) = self.nested.get_mut(parents.len()) else {
                return;
            };
            if *own_children != children || *own_position < position {
                return;
            }
            own_position
        } else {
            if self.root < at.root {
                return;
            }
            &mut self.root
        };
        if insert {
            *index += 1;
        } else {
            *index -= 1;
        }
    }

    pub(super) fn root(index: usize) -> Self {
        Self {
            root: index,
            nested: Vec::new(),
        }
    }

    pub(super) fn child(&self, children: Children, index: usize) -> Self {
        let mut path = self.clone();
        path.nested.push((children, index));
        path
    }

    pub(super) fn get<'doc, 'a>(&self, blocks: &'doc [Block<'a>]) -> Option<&'doc Block<'a>> {
        let mut block = blocks.get(self.root)?;
        for &(children, index) in &self.nested {
            block = children.get(block)?.get(index)?;
        }
        Some(block)
    }

    pub(super) fn get_mut<'doc, 'a>(
        &self,
        blocks: &'doc mut [Block<'a>],
    ) -> Option<&'doc mut Block<'a>> {
        let mut block = blocks.get_mut(self.root)?;
        for &(children, index) in &self.nested {
            block = children.get_mut(block)?.get_mut(index)?;
        }
        Some(block)
    }
}

impl Children {
    fn get<'doc, 'a>(self, block: &'doc Block<'a>) -> Option<&'doc [Block<'a>]> {
        match (self, block) {
            (Self::Body, Block::Section(block)) => Some(&block.content),
            (Self::Body, Block::Admonition(block)) => Some(&block.blocks),
            (Self::Body, Block::DelimitedBlock(block)) => compound(&block.inner),
            (Self::Item(index), Block::UnorderedList(block)) => {
                Some(&block.items.get(index)?.blocks)
            }
            (Self::Item(index), Block::OrderedList(block)) => Some(&block.items.get(index)?.blocks),
            (Self::Item(index), Block::CalloutList(block)) => Some(&block.items.get(index)?.blocks),
            (Self::Item(index), Block::DescriptionList(block)) => {
                Some(&block.items.get(index)?.definition.as_ref()?.blocks)
            }
            (Self::Cell { part, row, column }, Block::DelimitedBlock(block)) => {
                let DelimitedBlockType::DelimitedTable(table) = &block.inner else {
                    return None;
                };
                let rows = match part {
                    TablePart::Head => table.header.as_slice(),
                    TablePart::Body => &table.rows,
                    TablePart::Foot => table.footer.as_slice(),
                };
                Some(&rows.get(row)?.columns.get(column)?.content)
            }
            _ => None,
        }
    }

    fn get_mut<'doc, 'a>(self, block: &'doc mut Block<'a>) -> Option<&'doc mut Vec<Block<'a>>> {
        match (self, block) {
            (Self::Body, Block::Section(block)) => Some(&mut block.content),
            (Self::Body, Block::Admonition(block)) => Some(&mut block.blocks),
            (Self::Body, Block::DelimitedBlock(block)) => match &mut block.inner {
                DelimitedBlockType::DelimitedExample(blocks)
                | DelimitedBlockType::DelimitedOpen(blocks)
                | DelimitedBlockType::DelimitedSidebar(blocks)
                | DelimitedBlockType::DelimitedQuote(blocks) => Some(blocks),
                DelimitedBlockType::DelimitedComment(_)
                | DelimitedBlockType::DelimitedListing(_)
                | DelimitedBlockType::DelimitedLiteral(_)
                | DelimitedBlockType::DelimitedTable(_)
                | DelimitedBlockType::DelimitedPass(_)
                | DelimitedBlockType::DelimitedVerse(_)
                | DelimitedBlockType::DelimitedStem(_) => None,
            },
            (Self::Item(index), Block::UnorderedList(block)) => {
                Some(&mut block.items.get_mut(index)?.blocks)
            }
            (Self::Item(index), Block::OrderedList(block)) => {
                Some(&mut block.items.get_mut(index)?.blocks)
            }
            (Self::Item(index), Block::CalloutList(block)) => {
                Some(&mut block.items.get_mut(index)?.blocks)
            }
            (Self::Item(index), Block::DescriptionList(block)) => {
                Some(&mut block.items.get_mut(index)?.definition.as_mut()?.blocks)
            }
            (Self::Cell { part, row, column }, Block::DelimitedBlock(block)) => {
                let DelimitedBlockType::DelimitedTable(table) = &mut block.inner else {
                    return None;
                };
                let rows = match part {
                    TablePart::Head => table.header.as_mut_slice(),
                    TablePart::Body => &mut table.rows,
                    TablePart::Foot => table.footer.as_mut_slice(),
                };
                Some(&mut rows.get_mut(row)?.columns.get_mut(column)?.content)
            }
            _ => None,
        }
    }
}

fn compound<'doc, 'a>(inner: &'doc DelimitedBlockType<'a>) -> Option<&'doc [Block<'a>]> {
    match inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => Some(blocks),
        DelimitedBlockType::DelimitedComment(_)
        | DelimitedBlockType::DelimitedListing(_)
        | DelimitedBlockType::DelimitedLiteral(_)
        | DelimitedBlockType::DelimitedTable(_)
        | DelimitedBlockType::DelimitedPass(_)
        | DelimitedBlockType::DelimitedVerse(_)
        | DelimitedBlockType::DelimitedStem(_) => None,
    }
}

pub(super) fn visit_children<'doc, 'a>(
    block: &'doc Block<'a>,
    mut visit: impl FnMut(Children, &'doc [Block<'a>]),
) {
    match block {
        Block::Section(block) => visit(Children::Body, &block.content),
        Block::Admonition(block) => visit(Children::Body, &block.blocks),
        Block::UnorderedList(block) => {
            for (index, item) in block.items.iter().enumerate() {
                visit(Children::Item(index), &item.blocks);
            }
        }
        Block::OrderedList(block) => {
            for (index, item) in block.items.iter().enumerate() {
                visit(Children::Item(index), &item.blocks);
            }
        }
        Block::CalloutList(block) => {
            for (index, item) in block.items.iter().enumerate() {
                visit(Children::Item(index), &item.blocks);
            }
        }
        Block::DescriptionList(block) => {
            for (index, item) in block.items.iter().enumerate() {
                if let Some(definition) = &item.definition {
                    visit(Children::Item(index), &definition.blocks);
                }
            }
        }
        Block::DelimitedBlock(block) => {
            if let Some(blocks) = compound(&block.inner) {
                visit(Children::Body, blocks);
            }
            if let DelimitedBlockType::DelimitedTable(table) = &block.inner {
                visit_table(table, visit);
            }
        }
        Block::TableOfContents(_)
        | Block::DiscreteHeading(_)
        | Block::DocumentAttribute(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::Paragraph(_)
        | Block::Image(_)
        | Block::Audio(_)
        | Block::Video(_)
        | Block::Comment(_)
        | Block::Extension(_) => {}
    }
}

fn visit_table<'doc, 'a>(
    table: &'doc Table<'a>,
    mut visit: impl FnMut(Children, &'doc [Block<'a>]),
) {
    for (part, rows) in [
        (TablePart::Head, table.header.as_slice()),
        (TablePart::Body, table.rows.as_slice()),
        (TablePart::Foot, table.footer.as_slice()),
    ] {
        for (row, value) in rows.iter().enumerate() {
            for (column, cell) in value.columns.iter().enumerate() {
                if cell.style == Some(ColumnStyle::AsciiDoc) {
                    visit(Children::Cell { part, row, column }, &cell.content);
                }
            }
        }
    }
}
