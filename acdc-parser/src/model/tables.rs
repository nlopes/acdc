//! Table types for `AsciiDoc` documents.

use serde::Serialize;

use super::location::Location;
use super::{AttributeValue, Block, BlockMetadata, DocumentAttributes};

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
    pub fn from_attributes(
        metadata: &BlockMetadata<'_>,
        attributes: &DocumentAttributes<'_>,
    ) -> Self {
        Self {
            frame: resolve_table_attribute(metadata, attributes, "frame", "table-frame").map_or(
                TableFrame::All,
                |value| match value {
                    "all" => TableFrame::All,
                    "ends" | "topbot" => TableFrame::Ends,
                    "sides" => TableFrame::Sides,
                    _ => TableFrame::None,
                },
            ),
            grid: resolve_table_attribute(metadata, attributes, "grid", "table-grid").map_or(
                TableGrid::All,
                |value| match value {
                    "all" => TableGrid::All,
                    "rows" => TableGrid::Rows,
                    "cols" => TableGrid::Columns,
                    _ => TableGrid::None,
                },
            ),
            stripes: resolve_table_attribute(metadata, attributes, "stripes", "table-stripes")
                .map_or(TableStripes::None, |value| match value {
                    "all" => TableStripes::All,
                    "odd" => TableStripes::Odd,
                    "even" => TableStripes::Even,
                    "hover" => TableStripes::Hover,
                    _ => TableStripes::None,
                }),
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
    metadata: &'value BlockMetadata<'_>,
    attributes: &'value DocumentAttributes<'_>,
    local_name: &str,
    document_name: &str,
) -> Option<&'value str> {
    let value = metadata
        .attributes
        .get(local_name)
        .or_else(|| attributes.get(document_name))?;
    match value {
        AttributeValue::String(value) => Some(value),
        AttributeValue::Bool(true) => Some(""),
        AttributeValue::Bool(false) | AttributeValue::None => None,
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
    pub halign: HorizontalAlignment,
    #[serde(default, skip_serializing_if = "is_default_valign")]
    pub valign: VerticalAlignment,
    #[serde(default, skip_serializing_if = "is_default_width")]
    pub width: ColumnWidth,
    #[serde(default, skip_serializing_if = "is_default_style")]
    pub style: ColumnStyle,
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
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Table<'a> {
    pub header: Option<TableRow<'a>>,
    pub footer: Option<TableRow<'a>>,
    pub rows: Vec<TableRow<'a>>,
    /// Column format specification for each column (alignment, width, style)
    /// Skipped if all columns have default format
    #[serde(default, skip_serializing_if = "are_all_columns_default")]
    pub columns: Vec<ColumnFormat>,
    #[serde(skip)]
    presentation: Option<TablePresentation>,
    pub location: Location,
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
    pub(crate) fn with_presentation(mut self, presentation: TablePresentation) -> Self {
        self.presentation = Some(presentation);
        self
    }

    /// Table decoration that overrides converter defaults, if present.
    #[must_use]
    pub const fn presentation(&self) -> Option<TablePresentation> {
        self.presentation
    }
}

/// A row in a table, containing one or more columns (cells).
///
/// # Note on Field Name
///
/// The field is named `columns` (not `cells`) to align with the column-oriented
/// table model. Each `TableColumn` represents one cell in this row.
///
/// ```
/// # use acdc_parser::{TableRow, TableColumn};
/// fn count_cells(row: &TableRow) -> usize {
///     row.columns.len()  // Access cells via .columns
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct TableRow<'a> {
    /// The cells in this row (one per table column).
    pub columns: Vec<TableColumn<'a>>,
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
    pub content: Vec<Block<'a>>,
    /// Number of columns this cell spans (default 1).
    /// Specified in `AsciiDoc` with `n+|` syntax (e.g., `2+|` for colspan=2).
    #[serde(skip_serializing_if = "is_default_span")]
    pub colspan: usize,
    /// Number of rows this cell spans (default 1).
    /// Specified in `AsciiDoc` with `.n+|` syntax (e.g., `.2+|` for rowspan=2).
    #[serde(skip_serializing_if = "is_default_span")]
    pub rowspan: usize,
    /// Cell-level horizontal alignment override.
    /// Specified with `<`, `^`, or `>` in cell specifier (e.g., `^|` for center).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub halign: Option<HorizontalAlignment>,
    /// Cell-level vertical alignment override.
    /// Specified with `.<`, `.^`, or `.>` in cell specifier (e.g., `.>|` for bottom).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valign: Option<VerticalAlignment>,
    /// Cell-level style override.
    /// Specified with style letter after operator (e.g., `s|` for strong/bold).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<ColumnStyle>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_default_span(span: &usize) -> bool {
    *span == 1
}

impl<'a> TableColumn<'a> {
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
            content,
            colspan,
            rowspan,
            halign,
            valign,
            style,
        }
    }
}
