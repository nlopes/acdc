//! Table types for `AsciiDoc` documents.

use serde::{Deserialize, Serialize};

use super::Block;
use super::location::Location;

/// Horizontal alignment for table cells
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HorizontalAlignment {
    #[default]
    Left,
    Center,
    Right,
}

/// Vertical alignment for table cells
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerticalAlignment {
    #[default]
    Top,
    Middle,
    Bottom,
}

/// Column width specification
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

/// A `TableRow` represents a row in a table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TableRow {
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TableColumn {
    pub content: Vec<Block>,
}

impl TableColumn {
    /// Create a new table column with the given content.
    #[must_use]
    pub fn new(content: Vec<Block>) -> Self {
        Self { content }
    }
}
