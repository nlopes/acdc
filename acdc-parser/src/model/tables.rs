//! Table types for `AsciiDoc` documents.

use serde::{Serialize, Serializer};

use crate::ast::AttributeValueRef;

use super::{
    AttributeName, Block, BlockMetadata, DocumentAttributeAssignment, FootnoteDefinition,
    location::Location,
};

/// The outer border applied to a table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum TableFrame {
    /// Draw all four outer edges.
    #[default]
    All,
    /// Draw only the top and bottom edges.
    Ends,
    /// Draw only the left and right edges.
    Sides,
    /// Draw no outer edge.
    None,
}

/// The rules drawn between table cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum TableGrid {
    /// Draw rules between rows and columns.
    #[default]
    All,
    /// Draw rules only between rows.
    Rows,
    /// Draw rules only between columns.
    Columns,
    /// Draw no rules between cells.
    None,
}

/// The body rows that receive a background fill.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum TableStripes {
    /// Do not fill body rows.
    #[default]
    None,
    /// Fill every body row.
    All,
    /// Fill odd body rows.
    Odd,
    /// Fill even body rows.
    Even,
    /// Fill the body row under the pointer in interactive output.
    Hover,
}

/// Table decoration resolved from the attributes at the table's source position.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct TablePresentation {
    frame: TableFrame,
    grid: TableGrid,
    stripes: TableStripes,
}

impl TablePresentation {
    /// Resolve local table decoration and the document defaults in effect.
    #[must_use]
    pub fn from_attributes<'value>(
        metadata: &'value BlockMetadata<'_>,
        lookup: impl FnMut(&str) -> Option<&'value crate::DocumentAttributeValue<'value>>,
    ) -> Self {
        Self::from_attribute_values(
            |name| metadata.attributes.get(name).map(AttributeValueRef::from),
            lookup,
        )
    }

    pub(crate) fn from_attribute_values<'value>(
        mut local: impl FnMut(&str) -> Option<AttributeValueRef<'value>>,
        mut lookup: impl FnMut(&str) -> Option<&'value crate::DocumentAttributeValue<'value>>,
    ) -> Self {
        Self {
            frame: resolve_table_attribute(local("frame"), &mut lookup, "table-frame").map_or(
                TableFrame::All,
                |value| match value {
                    "all" => TableFrame::All,
                    "ends" | "topbot" => TableFrame::Ends,
                    "sides" => TableFrame::Sides,
                    _ => TableFrame::None,
                },
            ),
            grid: resolve_table_attribute(local("grid"), &mut lookup, "table-grid").map_or(
                TableGrid::All,
                |value| match value {
                    "all" => TableGrid::All,
                    "rows" => TableGrid::Rows,
                    "cols" => TableGrid::Columns,
                    _ => TableGrid::None,
                },
            ),
            stripes: resolve_table_attribute(local("stripes"), &mut lookup, "table-stripes")
                .map_or(TableStripes::None, |value| match value {
                    "all" => TableStripes::All,
                    "odd" => TableStripes::Odd,
                    "even" => TableStripes::Even,
                    "hover" => TableStripes::Hover,
                    _ => TableStripes::None,
                }),
        }
    }

    fn with_local_attributes(self, metadata: &BlockMetadata<'_>) -> Self {
        let local = Self::from_attributes(metadata, |_| None);
        Self {
            frame: if metadata.attributes.contains_key("frame") {
                local.frame
            } else {
                self.frame
            },
            grid: if metadata.attributes.contains_key("grid") {
                local.grid
            } else {
                self.grid
            },
            stripes: if metadata.attributes.contains_key("stripes") {
                local.stripes
            } else {
                self.stripes
            },
        }
    }

    /// The outer border applied to the table.
    #[must_use]
    pub const fn frame(self) -> TableFrame {
        self.frame
    }

    /// The rules drawn between table cells.
    #[must_use]
    pub const fn grid(self) -> TableGrid {
        self.grid
    }

    /// The body rows that receive a background fill.
    #[must_use]
    pub const fn stripes(self) -> TableStripes {
        self.stripes
    }
}

fn resolve_table_attribute<'value>(
    local: Option<AttributeValueRef<'value>>,
    lookup: &mut impl FnMut(&str) -> Option<&'value crate::DocumentAttributeValue<'value>>,
    document_name: &str,
) -> Option<&'value str> {
    if let Some(value) = local {
        return match value {
            AttributeValueRef::Text(value) => Some(value),
            AttributeValueRef::Bool(true) => Some(""),
            AttributeValueRef::Bool(false)
            | AttributeValueRef::None
            | AttributeValueRef::Content(_)
            | AttributeValueRef::ContentList(_)
            | AttributeValueRef::Tokens(_)
            | AttributeValueRef::Substitutions(_) => None,
        };
    }
    let value = lookup(document_name)?;
    if value.is_presence() {
        Some("")
    } else {
        value.text()
    }
}

/// Horizontal alignment for table cells
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HorizontalAlignment {
    #[default]
    Left,
    Center,
    Right,
}

/// Vertical alignment for table cells
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VerticalAlignment {
    #[default]
    Top,
    Middle,
    Bottom,
}

/// Column width specification
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ColumnWidth {
    /// Proportional width (e.g., 1, 2, 3 - relative to other columns)
    Proportional(u32),
    /// Percentage width (e.g., 15%, 30%, 55%)
    Percentage(u32),
    /// Auto-width - content determines width (~)
    Auto,
}

impl Default for ColumnWidth {
    fn default() -> Self {
        ColumnWidth::Proportional(1)
    }
}

/// Column content style
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ColumnStyle {
    /// `AsciiDoc` block content (a) - supports lists, blocks, macros
    #[serde(rename = "asciidoc")]
    AsciiDoc,
    /// Default paragraph-level markup (d)
    #[default]
    Default,
    /// Emphasis/italic (e)
    Emphasis,
    /// Header styling (h)
    Header,
    /// Literal block text (l)
    Literal,
    /// Monospace font (m)
    Monospace,
    /// Strong/bold (s)
    Strong,
}

/// Column format specification for table formatting
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[non_exhaustive]
pub struct ColumnFormat {
    #[serde(default, skip_serializing_if = "is_default_halign")]
    pub(crate) halign: HorizontalAlignment,
    #[serde(default, skip_serializing_if = "is_default_valign")]
    pub(crate) valign: VerticalAlignment,
    #[serde(default, skip_serializing_if = "is_default_width")]
    pub(crate) width: ColumnWidth,
    #[serde(default, skip_serializing_if = "is_default_style")]
    pub(crate) style: ColumnStyle,
}

impl ColumnFormat {
    /// Create a new column format with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the horizontal alignment.
    #[must_use]
    pub fn with_halign(mut self, halign: HorizontalAlignment) -> Self {
        self.halign = halign;
        self
    }

    /// Set the vertical alignment.
    #[must_use]
    pub fn with_valign(mut self, valign: VerticalAlignment) -> Self {
        self.valign = valign;
        self
    }

    /// Set the column width.
    #[must_use]
    pub fn with_width(mut self, width: ColumnWidth) -> Self {
        self.width = width;
        self
    }

    /// Set the column style.
    #[must_use]
    pub fn with_style(mut self, style: ColumnStyle) -> Self {
        self.style = style;
        self
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_default_halign(h: &HorizontalAlignment) -> bool {
    *h == HorizontalAlignment::default()
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_default_valign(v: &VerticalAlignment) -> bool {
    *v == VerticalAlignment::default()
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_default_width(w: &ColumnWidth) -> bool {
    *w == ColumnWidth::default()
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_default_style(s: &ColumnStyle) -> bool {
    *s == ColumnStyle::default()
}

pub(crate) fn are_all_columns_default(specs: &[ColumnFormat]) -> bool {
    specs.iter().all(|s| *s == ColumnFormat::default())
}

/// A `Table` represents a table in a document.
///
/// Use [`Self::take_rows`] and [`Self::set_rows`] for owned row edits.
///
/// ```compile_fail,E0616
/// use acdc_parser::{Location, Table};
/// let mut table = Table::new(Vec::new(), Location::default());
/// table.rows.clear();
/// ```
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Table<'a> {
    pub(crate) header: Option<TableRow<'a>>,
    pub(crate) footer: Option<TableRow<'a>>,
    pub(crate) rows: Vec<TableRow<'a>>,
    /// Column format specification for each column (alignment, width, style)
    /// Skipped if all columns have default format
    #[serde(default, skip_serializing_if = "are_all_columns_default")]
    pub(crate) columns: Vec<ColumnFormat>,
    #[serde(skip)]
    presentation: Option<ResolvedTablePresentation>,
    pub(crate) location: Location,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ResolvedTablePresentation {
    defaults: TablePresentation,
    value: TablePresentation,
}

impl<'a> Table<'a> {
    /// Create a new table with the given rows and location.
    #[must_use]
    pub fn new(rows: Vec<TableRow<'a>>, location: Location) -> Self {
        Self {
            header: None,
            footer: None,
            rows,
            columns: Vec::new(),
            presentation: None,
            location,
        }
    }

    /// Set the header row.
    #[must_use]
    pub fn with_header(mut self, header: Option<TableRow<'a>>) -> Self {
        self.header = header;
        self
    }

    /// Set the footer row.
    #[must_use]
    pub fn with_footer(mut self, footer: Option<TableRow<'a>>) -> Self {
        self.footer = footer;
        self
    }

    /// Set the column format specifications.
    #[must_use]
    pub fn with_columns(mut self, columns: Vec<ColumnFormat>) -> Self {
        self.columns = columns;
        self
    }

    #[must_use]
    pub(crate) fn with_presentation(
        mut self,
        metadata: &BlockMetadata<'_>,
        defaults: TablePresentation,
    ) -> Self {
        self.presentation = Some(ResolvedTablePresentation {
            defaults,
            value: defaults.with_local_attributes(metadata),
        });
        self
    }

    pub(crate) fn refresh_presentation(&mut self, metadata: &BlockMetadata<'_>) {
        if let Some(presentation) = &mut self.presentation {
            presentation.value = presentation.defaults.with_local_attributes(metadata);
        }
    }

    /// Table decoration that overrides converter defaults, if present.
    #[must_use]
    pub const fn presentation(&self) -> Option<TablePresentation> {
        match &self.presentation {
            Some(presentation) => Some(presentation.value),
            None => None,
        }
    }
}

/// A row in a table, containing one or more columns (cells).
///
/// ```
/// # use acdc_parser::ast::TableRowRef;
/// fn count_cells(row: TableRowRef<'_>) -> usize {
///     row.cells().count()
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct TableRow<'a> {
    /// The cells in this row (one per table column).
    pub(crate) columns: Vec<TableColumn<'a>>,
}

impl<'a> TableRow<'a> {
    /// Create a new table row with the given columns.
    #[must_use]
    pub fn new(columns: Vec<TableColumn<'a>>) -> Self {
        Self { columns }
    }
}

/// A `TableColumn` represents a column/cell in a table row.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct TableColumn<'a> {
    // Native occurrences own bodies; this marker preserves the catalog boundary.
    #[serde(
        rename = "footnote_scope",
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_footnote_scope"
    )]
    pub(crate) footnotes: Option<Vec<FootnoteDefinition<'a>>>,
    #[serde(skip)]
    pub(crate) initial_attributes: Vec<(AttributeName<'a>, DocumentAttributeAssignment<'a>)>,
    pub(crate) content: Vec<Block<'a>>,
    /// Number of columns this cell spans (default 1).
    /// Specified in `AsciiDoc` with `n+|` syntax (e.g., `2+|` for colspan=2).
    #[serde(skip_serializing_if = "is_default_span")]
    pub(crate) colspan: usize,
    /// Number of rows this cell spans (default 1).
    /// Specified in `AsciiDoc` with `.n+|` syntax (e.g., `.2+|` for rowspan=2).
    #[serde(skip_serializing_if = "is_default_span")]
    pub(crate) rowspan: usize,
    /// Cell-level horizontal alignment override.
    /// Specified with `<`, `^`, or `>` in cell specifier (e.g., `^|` for center).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) halign: Option<HorizontalAlignment>,
    /// Cell-level vertical alignment override.
    /// Specified with `.<`, `.^`, or `.>` in cell specifier (e.g., `.>|` for bottom).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) valign: Option<VerticalAlignment>,
    /// Cell-level style override.
    /// Specified with style letter after operator (e.g., `s|` for strong/bold).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) style: Option<ColumnStyle>,
}

#[allow(
    clippy::ref_option,
    reason = "serde passes the original field by reference"
)]
fn serialize_footnote_scope<S: Serializer>(
    scope: &Option<Vec<FootnoteDefinition<'_>>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_bool(scope.is_some())
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_default_span(span: &usize) -> bool {
    *span == 1
}

impl<'a> TableColumn<'a> {
    /// Attribute assignments applied before this `AsciiDoc` cell's content.
    ///
    /// These initialize the nested scope; they do not change the parent document.
    pub fn initial_attributes(
        &self,
    ) -> impl Iterator<Item = (&str, &DocumentAttributeAssignment<'a>)> {
        self.initial_attributes
            .iter()
            .map(|(name, assignment)| (name.as_ref(), assignment))
    }

    /// Create a new table column with full cell specifier options.
    #[must_use]
    pub(crate) fn with_format(
        content: Vec<Block<'a>>,
        colspan: usize,
        rowspan: usize,
        halign: Option<HorizontalAlignment>,
        valign: Option<VerticalAlignment>,
        style: Option<ColumnStyle>,
    ) -> Self {
        Self {
            initial_attributes: Vec::new(),
            footnotes: None,
            content,
            colspan,
            rowspan,
            halign,
            valign,
            style,
        }
    }
}

impl<'a> Table<'a> {
    /// Replace the header, preserving a present empty row.
    pub fn set_header(&mut self, header: Option<TableRow<'a>>) {
        self.header = header;
    }

    /// Remove the header without moving a body row into its place.
    #[must_use]
    pub fn take_header(&mut self) -> Option<TableRow<'a>> {
        self.header.take()
    }

    /// Replace the footer, preserving a present empty row.
    pub fn set_footer(&mut self, footer: Option<TableRow<'a>>) {
        self.footer = footer;
    }

    /// Remove the footer without moving a body row into its place.
    #[must_use]
    pub fn take_footer(&mut self) -> Option<TableRow<'a>> {
        self.footer.take()
    }

    /// Replace prepared body rows without changing header, footer or presentation.
    pub fn set_rows(&mut self, rows: impl IntoIterator<Item = TableRow<'a>>) {
        self.rows = rows.into_iter().collect();
    }

    /// Append a body row without recalculating cell positions or spans.
    pub fn push_row(&mut self, row: TableRow<'a>) {
        self.rows.push(row);
    }

    /// Remove body rows, retaining their content and original source lifetimes.
    pub fn take_rows(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = TableRow<'a>> + ExactSizeIterator + use<'a> {
        std::mem::take(&mut self.rows).into_iter()
    }

    /// Replace column formats without reinterpreting existing cell content.
    pub fn set_columns(&mut self, columns: impl IntoIterator<Item = ColumnFormat>) {
        self.columns = columns.into_iter().collect();
    }

    /// Remove column formats without changing cell overrides or resolved content.
    pub fn take_columns(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = ColumnFormat> + ExactSizeIterator + use<> {
        std::mem::take(&mut self.columns).into_iter()
    }

    /// Replace the table body's span without relocating its cells or enclosing block.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }
}

impl<'a> TableRow<'a> {
    /// Replace prepared cells in row order without filling span placeholders.
    pub fn set_cells(&mut self, cells: impl IntoIterator<Item = TableColumn<'a>>) {
        self.columns = cells.into_iter().collect();
    }

    /// Append a cell without recalculating neighboring cell positions.
    pub fn push_cell(&mut self, cell: TableColumn<'a>) {
        self.columns.push(cell);
    }

    /// Remove cells, retaining their original source lifetimes and scope snapshots.
    pub fn take_cells(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = TableColumn<'a>> + ExactSizeIterator + use<'a> {
        std::mem::take(&mut self.columns).into_iter()
    }
}

impl<'a> TableColumn<'a> {
    /// Replace this cell's prepared footnote catalog without registering occurrences.
    /// `Some`, including an empty catalog, makes the cell an independent owner.
    /// `None` uses the containing owner. Style changes do not change this boundary.
    pub fn set_footnotes(&mut self, definitions: Option<Vec<FootnoteDefinition<'a>>>) {
        self.footnotes = definitions;
    }

    /// Remove the cell's footnote ownership and return its retained definitions.
    #[must_use]
    pub fn take_footnotes(&mut self) -> Option<Vec<FootnoteDefinition<'a>>> {
        self.footnotes.take()
    }

    /// Construct an unspanned cell from prepared blocks, without a nested scope snapshot.
    #[must_use]
    pub fn new(content: Vec<Block<'a>>) -> Self {
        Self::with_format(content, 1, 1, None, None, None)
    }

    /// Replace prepared blocks without replaying the cell's initial assignments.
    pub fn set_blocks(&mut self, content: impl IntoIterator<Item = Block<'a>>) {
        self.content = content.into_iter().collect();
    }

    /// Append a prepared block without replaying assignments or registering content.
    pub fn push_block(&mut self, block: Block<'a>) {
        self.content.push(block);
    }

    /// Remove blocks, retaining their original source lifetimes and the cell's scope snapshot.
    pub fn take_blocks(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = Block<'a>> + ExactSizeIterator + use<'a> {
        std::mem::take(&mut self.content).into_iter()
    }

    /// Replace the retained column span without rebuilding the table grid.
    pub fn set_colspan(&mut self, colspan: usize) {
        self.colspan = colspan;
    }

    /// Replace the retained row span without rebuilding the table grid.
    pub fn set_rowspan(&mut self, rowspan: usize) {
        self.rowspan = rowspan;
    }

    /// Replace the cell's horizontal alignment override.
    pub fn set_halign(&mut self, halign: Option<HorizontalAlignment>) {
        self.halign = halign;
    }

    /// Replace the cell's vertical alignment override.
    pub fn set_valign(&mut self, valign: Option<VerticalAlignment>) {
        self.valign = valign;
    }

    /// Replace the retained style without reinterpreting already prepared content.
    pub fn set_style(&mut self, style: Option<ColumnStyle>) {
        self.style = style;
    }
}
