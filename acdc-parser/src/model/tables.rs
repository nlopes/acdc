//! Table types for `AsciiDoc` documents.

use serde::Serialize;

use super::Block;
use super::location::Location;

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
pub struct Table {
    pub header: Option<TableRow>,
    pub footer: Option<TableRow>,
    pub rows: Vec<TableRow>,
    /// Column format specification for each column (alignment, width, style)
    /// Skipped if all columns have default format
    #[serde(default, skip_serializing_if = "are_all_columns_default")]
    pub columns: Vec<ColumnFormat>,
    pub location: Location,
}

impl Table {
    /// Create a new table with the given rows and location.
    #[must_use]
    pub fn new(rows: Vec<TableRow>, location: Location) -> Self {
        Self {
            header: None,
            footer: None,
            rows,
            columns: Vec::new(),
            location,
        }
    }

    /// Set the header row.
    #[must_use]
    pub fn with_header(mut self, header: Option<TableRow>) -> Self {
        self.header = header;
        self
    }

    /// Set the footer row.
    #[must_use]
    pub fn with_footer(mut self, footer: Option<TableRow>) -> Self {
        self.footer = footer;
        self
    }

    /// Set the column format specifications.
    #[must_use]
    pub fn with_columns(mut self, columns: Vec<ColumnFormat>) -> Self {
        self.columns = columns;
        self
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
pub struct TableRow {
    /// The cells in this row (one per table column).
    pub columns: Vec<TableColumn>,
}

impl TableRow {
    /// Create a new table row with the given columns.
    #[must_use]
    pub fn new(columns: Vec<TableColumn>) -> Self {
        Self { columns }
    }
}

/// A `TableColumn` represents a column/cell in a table row.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct TableColumn {
    pub content: Vec<Block>,
    /// Number of columns this cell spans (default 1).
    /// Specified in `AsciiDoc` with `n+|` syntax (e.g., `2+|` for colspan=2).
    #[serde(default = "default_span", skip_serializing_if = "is_default_span")]
    pub colspan: usize,
    /// Number of rows this cell spans (default 1).
    /// Specified in `AsciiDoc` with `.n+|` syntax (e.g., `.2+|` for rowspan=2).
    #[serde(default = "default_span", skip_serializing_if = "is_default_span")]
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

const fn default_span() -> usize {
    1
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_default_span(span: &usize) -> bool {
    *span == 1
}

impl TableColumn {
    /// Create a new table column with the given content and default spans (1).
    #[must_use]
    pub fn new(content: Vec<Block>) -> Self {
        Self {
            content,
            colspan: 1,
            rowspan: 1,
            halign: None,
            valign: None,
            style: None,
        }
    }

    /// Create a new table column with content and explicit span values.
    #[must_use]
    pub fn with_spans(content: Vec<Block>, colspan: usize, rowspan: usize) -> Self {
        Self {
            content,
            colspan,
            rowspan,
            halign: None,
            valign: None,
            style: None,
        }
    }

    /// Create a new table column with full cell specifier options.
    #[must_use]
    pub fn with_format(
        content: Vec<Block>,
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
