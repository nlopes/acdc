use crate::{
    ColumnFormat, ColumnStyle, ColumnWidth, DelimitedBlock, HorizontalAlignment, Table,
    TableColumn, TablePresentation, TableRow, VerticalAlignment,
};

use super::{AttributeValueRef, BlockRef, ContentRef, ElementIdRef, LocationRef};

/// A table block, including its title and local declarations.
#[derive(Clone, Copy, Debug)]
pub struct TableRef<'doc> {
    block: &'doc DelimitedBlock<'doc>,
    table: &'doc Table<'doc>,
}

impl<'doc> TableRef<'doc> {
    pub(super) fn new(block: &'doc DelimitedBlock<'doc>, table: &'doc Table<'doc>) -> Self {
        Self { block, table }
    }

    /// The table's caption title, including an explicitly empty title.
    #[must_use]
    pub fn title(self) -> Option<ContentRef<'doc>> {
        self.block.title.content()
    }

    /// The primary ID, if present.
    #[must_use]
    pub fn id(self) -> Option<ElementIdRef<'doc>> {
        self.block
            .metadata
            .id_anchor()
            .map(|id| ElementIdRef::new(id.id, &id.location))
    }

    /// The source span of the complete table block.
    #[must_use]
    pub fn location(self) -> LocationRef<'doc> {
        LocationRef::from(&self.block.location)
    }

    /// The header row, including a present empty row.
    #[must_use]
    pub fn header(self) -> Option<TableRowRef<'doc>> {
        self.table.header.as_ref().map(TableRowRef::from)
    }

    /// Body rows in source order, excluding the header and footer.
    pub fn rows(self) -> impl Iterator<Item = TableRowRef<'doc>> + Send + Sync + 'doc {
        self.table.rows.iter().map(TableRowRef::from)
    }

    /// The footer row, including a present empty row.
    #[must_use]
    pub fn footer(self) -> Option<TableRowRef<'doc>> {
        self.table.footer.as_ref().map(TableRowRef::from)
    }

    /// Column formats in table order.
    pub fn columns(self) -> impl Iterator<Item = ColumnFormatRef<'doc>> + Send + Sync + 'doc {
        self.table.columns.iter().map(ColumnFormatRef::from)
    }

    /// Decoration resolved at this table's source position; absent on unresolved constructed tables.
    #[must_use]
    pub fn presentation(self) -> Option<TablePresentation> {
        self.table.presentation()
    }

    /// The local `frame` value, without document defaults or interpretation.
    #[must_use]
    pub fn declared_frame(self) -> Option<AttributeValueRef<'doc>> {
        self.block
            .metadata
            .attributes
            .get("frame")
            .map(AttributeValueRef::from)
    }

    /// The local `grid` value, without document defaults or interpretation.
    #[must_use]
    pub fn declared_grid(self) -> Option<AttributeValueRef<'doc>> {
        self.block
            .metadata
            .attributes
            .get("grid")
            .map(AttributeValueRef::from)
    }

    /// The local `stripes` value, without document defaults or interpretation.
    #[must_use]
    pub fn declared_stripes(self) -> Option<AttributeValueRef<'doc>> {
        self.block
            .metadata
            .attributes
            .get("stripes")
            .map(AttributeValueRef::from)
    }
}

/// A table row.
#[derive(Clone, Copy, Debug)]
pub struct TableRowRef<'doc> {
    row: &'doc TableRow<'doc>,
}

impl<'doc> TableRowRef<'doc> {
    /// Cells in row order. Spanning cells occur once, without synthetic placeholders.
    pub fn cells(self) -> impl Iterator<Item = TableCellRef<'doc>> + Send + Sync + 'doc {
        self.row.columns.iter().map(TableCellRef::from)
    }
}

impl<'doc> From<&'doc TableRow<'doc>> for TableRowRef<'doc> {
    fn from(row: &'doc TableRow<'doc>) -> Self {
        Self { row }
    }
}

/// A table cell and its parsed formatting.
#[derive(Clone, Copy, Debug)]
pub struct TableCellRef<'doc> {
    cell: &'doc TableColumn<'doc>,
}

impl<'doc> TableCellRef<'doc> {
    /// Direct blocks in cell order; their locations retain the original source coordinates.
    pub fn blocks(self) -> impl Iterator<Item = BlockRef<'doc>> + Send + Sync + 'doc {
        self.cell.content.iter().map(BlockRef::from)
    }

    /// The number of columns covered by this cell.
    #[must_use]
    pub fn colspan(self) -> usize {
        self.cell.colspan
    }

    /// The number of rows covered by this cell.
    #[must_use]
    pub fn rowspan(self) -> usize {
        self.cell.rowspan
    }

    /// The retained cell-level horizontal alignment.
    #[must_use]
    pub fn halign(self) -> Option<HorizontalAlignment> {
        self.cell.halign
    }

    /// The retained cell-level vertical alignment.
    #[must_use]
    pub fn valign(self) -> Option<VerticalAlignment> {
        self.cell.valign
    }

    /// The cell's resolved content style, when present.
    #[must_use]
    pub fn style(self) -> Option<ColumnStyle> {
        self.cell.style
    }
}

impl<'doc> From<&'doc TableColumn<'doc>> for TableCellRef<'doc> {
    fn from(cell: &'doc TableColumn<'doc>) -> Self {
        Self { cell }
    }
}

/// A table column's format.
#[derive(Clone, Copy, Debug)]
pub struct ColumnFormatRef<'doc> {
    format: &'doc ColumnFormat,
}

impl ColumnFormatRef<'_> {
    /// The column's horizontal alignment.
    #[must_use]
    pub fn halign(self) -> HorizontalAlignment {
        self.format.halign
    }

    /// The column's vertical alignment.
    #[must_use]
    pub fn valign(self) -> VerticalAlignment {
        self.format.valign
    }

    /// The column's relative, percentage or automatic width.
    #[must_use]
    pub fn width(self) -> ColumnWidth {
        self.format.width
    }

    /// The column's content style.
    #[must_use]
    pub fn style(self) -> ColumnStyle {
        self.format.style
    }
}

impl<'doc> From<&'doc ColumnFormat> for ColumnFormatRef<'doc> {
    fn from(format: &'doc ColumnFormat) -> Self {
        Self { format }
    }
}
