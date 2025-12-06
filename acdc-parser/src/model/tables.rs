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

/// A `TableRow` represents a row in a table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableRow {
    pub columns: Vec<TableColumn>,
}

/// A `TableColumn` represents a column/cell in a table row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableColumn {
    pub content: Vec<Block>,
}
