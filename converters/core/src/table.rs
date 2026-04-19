//! Table utilities shared between converters.
//!
//! The grid utilities ([`build_grid`], [`CellKind`], [`GridRow`]) are for output
//! formats that lack native colspan/rowspan support and need to reconstruct cell
//! positions themselves (e.g. manpage tbl, terminal). Formats with native span
//! support (like HTML, which uses `<td colspan="N">`) iterate the AST cells
//! directly — the grid would add unnecessary indirection.

use acdc_parser::{ColumnFormat, ColumnWidth, Table, TableRow};

/// Calculate column widths as percentages from column format specifications.
///
/// Converts proportional widths (e.g., `1,3`) to percentages (e.g., `25%, 75%`).
/// Percentage widths are passed through directly. Auto widths return 0.0,
/// leaving the renderer to decide.
///
/// # Arguments
///
/// * `columns` - Slice of column format specifications from the table
///
/// # Returns
///
/// Vector of width percentages. Empty if input is empty.
///
/// # Examples
///
/// ```ignore
/// // [cols="1,3"] -> [25.0, 75.0]
/// // [cols="2,1,1,1,1"] -> [33.33, 16.67, 16.67, 16.67, 16.67]
/// // [cols="25%,75%"] -> [25.0, 75.0]
/// ```
#[must_use]
pub fn calculate_column_widths(columns: &[ColumnFormat]) -> Vec<f64> {
    if columns.is_empty() {
        return vec![];
    }

    // Sum all proportional widths
    let total_proportional: u32 = columns
        .iter()
        .filter_map(|c| match c.width {
            ColumnWidth::Proportional(w) => Some(w),
            ColumnWidth::Percentage(_) | ColumnWidth::Auto | _ => None,
        })
        .sum();

    // Calculate percentage for each column
    let mut widths: Vec<f64> = columns
        .iter()
        .map(|c| match c.width {
            ColumnWidth::Proportional(w) if total_proportional > 0 => {
                (f64::from(w) / f64::from(total_proportional)) * 100.0
            }
            ColumnWidth::Percentage(p) => f64::from(p),
            // No proportional context, auto, or unknown width - let renderer decide
            ColumnWidth::Proportional(_) | ColumnWidth::Auto | _ => 0.0,
        })
        .collect();

    // Normalize percentage widths to sum to 100% (like asciidoctor does).
    // Only non-zero (non-auto) widths participate in normalization.
    let pct_sum: f64 = widths.iter().filter(|w| **w > 0.0).sum();
    if pct_sum > 0.0 && (pct_sum - 100.0).abs() > f64::EPSILON {
        let scale = 100.0 / pct_sum;
        for w in &mut widths {
            if *w > 0.0 {
                *w *= scale;
            }
        }
    }

    widths
}

/// What occupies a logical cell position in a table grid.
///
/// Used by converters that lack native colspan/rowspan support to build a
/// normalized grid where every row has the same number of columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellKind {
    /// A real cell with content. `cell_index` indexes into the AST row's `columns` vec.
    Content {
        /// Index into the AST row's `columns` vector.
        cell_index: usize,
    },
    /// Horizontal span placeholder (the primary cell is to the left).
    HSpan,
    /// Vertical span placeholder (the primary cell is above).
    VSpan,
}

/// A logical row in the grid, with metadata about its role.
#[derive(Debug)]
pub struct GridRow<'a> {
    /// The cell kinds for each logical column position.
    pub cells: Vec<CellKind>,
    /// Reference to the original AST row.
    pub ast_row: &'a TableRow<'a>,
    /// Whether this row is a header row.
    pub is_header: bool,
    /// Whether this row is a footer row.
    pub is_footer: bool,
}

/// Determine the true logical column count, accounting for colspan.
///
/// If the table has explicit column definitions, returns that count.
/// Otherwise, scans all rows and returns the maximum colspan-adjusted width.
#[must_use]
pub fn determine_column_count(table: &Table) -> usize {
    if !table.columns.is_empty() {
        return table.columns.len();
    }

    let all_rows = table
        .header
        .iter()
        .chain(table.rows.iter())
        .chain(table.footer.iter());

    all_rows
        .map(|row| row.columns.iter().map(|c| c.colspan.max(1)).sum::<usize>())
        .max()
        .unwrap_or(1)
}

/// Build a logical grid from all table rows, normalizing spans into a
/// rectangular grid where every row has exactly `num_cols` entries.
///
/// Each cell position is either a real content cell, a horizontal span
/// placeholder, or a vertical span placeholder.
#[must_use]
pub fn build_grid<'a>(table: &'a Table<'a>, num_cols: usize) -> Vec<GridRow<'a>> {
    let all_rows: Vec<(&'a TableRow<'a>, bool, bool)> = table
        .header
        .iter()
        .map(|r| (r, true, false))
        .chain(table.rows.iter().map(|r| (r, false, false)))
        .chain(table.footer.iter().map(|r| (r, false, true)))
        .collect();

    let mut grid = Vec::with_capacity(all_rows.len());
    let mut rowspan_remaining = vec![0usize; num_cols];

    for (ast_row, is_header, is_footer) in &all_rows {
        let mut row_cells = Vec::with_capacity(num_cols);
        let mut cell_cursor = 0;
        let mut col = 0;

        while col < num_cols {
            if let Some(remaining) = rowspan_remaining.get_mut(col)
                && *remaining > 0
            {
                row_cells.push(CellKind::VSpan);
                *remaining -= 1;
                col += 1;
                continue;
            }

            let Some(cell) = ast_row.columns.get(cell_cursor) else {
                // Shouldn't happen in well-formed input; fill remaining
                row_cells.push(CellKind::HSpan);
                col += 1;
                continue;
            };

            let colspan = cell.colspan.max(1);
            let rowspan = cell.rowspan.max(1);

            row_cells.push(CellKind::Content {
                cell_index: cell_cursor,
            });

            // Fill horizontal span markers for extra colspan columns
            for _ in 1..colspan {
                if row_cells.len() < num_cols {
                    row_cells.push(CellKind::HSpan);
                }
            }

            // Set rowspan tracking for all columns this cell covers
            for i in 0..colspan {
                if let Some(remaining) = rowspan_remaining.get_mut(col + i) {
                    *remaining = rowspan - 1;
                }
            }

            col += colspan;
            cell_cursor += 1;
        }

        grid.push(GridRow {
            cells: row_cells,
            ast_row,
            is_header: *is_header,
            is_footer: *is_footer,
        });
    }

    grid
}

/// Check whether any cell in the table has colspan or rowspan greater than 1.
#[must_use]
pub fn table_has_spans(table: &Table) -> bool {
    let all_rows = table
        .header
        .iter()
        .chain(table.rows.iter())
        .chain(table.footer.iter());

    all_rows
        .flat_map(|row| &row.columns)
        .any(|cell| cell.colspan > 1 || cell.rowspan > 1)
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn make_column(width: ColumnWidth) -> ColumnFormat {
        ColumnFormat::new().with_width(width)
    }

    #[test]
    fn test_proportional_widths() {
        let columns = vec![
            make_column(ColumnWidth::Proportional(1)),
            make_column(ColumnWidth::Proportional(3)),
        ];
        let widths = calculate_column_widths(&columns);
        assert_eq!(widths.len(), 2);
        assert!((widths[0] - 25.0).abs() < 0.01);
        assert!((widths[1] - 75.0).abs() < 0.01);
    }

    #[test]
    fn test_equal_proportional_widths() {
        let columns = vec![
            make_column(ColumnWidth::Proportional(1)),
            make_column(ColumnWidth::Proportional(1)),
            make_column(ColumnWidth::Proportional(1)),
        ];
        let widths = calculate_column_widths(&columns);
        assert_eq!(widths.len(), 3);
        for w in &widths {
            assert!((*w - 33.333).abs() < 0.01);
        }
    }

    #[test]
    fn test_percentage_widths() {
        let columns = vec![
            make_column(ColumnWidth::Percentage(25)),
            make_column(ColumnWidth::Percentage(75)),
        ];
        let widths = calculate_column_widths(&columns);
        assert_eq!(widths, vec![25.0, 75.0]);
    }

    #[test]
    fn test_auto_widths() {
        let columns = vec![
            make_column(ColumnWidth::Auto),
            make_column(ColumnWidth::Auto),
        ];
        let widths = calculate_column_widths(&columns);
        assert_eq!(widths, vec![0.0, 0.0]);
    }

    #[test]
    fn test_empty_columns() {
        let widths = calculate_column_widths(&[]);
        assert!(widths.is_empty());
    }

    #[test]
    fn test_percentage_over_100_normalized() {
        // [cols="34%,36%,31%"] sums to 101% → normalize to 100%
        let columns = vec![
            make_column(ColumnWidth::Percentage(34)),
            make_column(ColumnWidth::Percentage(36)),
            make_column(ColumnWidth::Percentage(31)),
        ];
        let widths = calculate_column_widths(&columns);
        let sum: f64 = widths.iter().sum();
        assert!((sum - 100.0).abs() < 0.01, "sum was {sum}");
        // Each scaled by 100/101
        assert!((widths[0] - 34.0 * 100.0 / 101.0).abs() < 0.01);
        assert!((widths[1] - 36.0 * 100.0 / 101.0).abs() < 0.01);
        assert!((widths[2] - 31.0 * 100.0 / 101.0).abs() < 0.01);
    }

    #[test]
    fn test_percentage_under_100_normalized() {
        // [cols="20%,30%,40%"] sums to 90% → normalize to 100%
        let columns = vec![
            make_column(ColumnWidth::Percentage(20)),
            make_column(ColumnWidth::Percentage(30)),
            make_column(ColumnWidth::Percentage(40)),
        ];
        let widths = calculate_column_widths(&columns);
        let sum: f64 = widths.iter().sum();
        assert!((sum - 100.0).abs() < 0.01, "sum was {sum}");
        assert!((widths[0] - 20.0 * 100.0 / 90.0).abs() < 0.01);
        assert!((widths[1] - 30.0 * 100.0 / 90.0).abs() < 0.01);
        assert!((widths[2] - 40.0 * 100.0 / 90.0).abs() < 0.01);
    }

    #[test]
    fn test_percentage_exact_100_unchanged() {
        let columns = vec![
            make_column(ColumnWidth::Percentage(25)),
            make_column(ColumnWidth::Percentage(25)),
            make_column(ColumnWidth::Percentage(50)),
        ];
        let widths = calculate_column_widths(&columns);
        assert_eq!(widths, vec![25.0, 25.0, 50.0]);
    }

    #[test]
    fn test_auto_with_percentage_over_100() {
        // [cols="~,60%,50%"] non-auto sum=110% → normalize percentages, auto stays 0.0
        let columns = vec![
            make_column(ColumnWidth::Auto),
            make_column(ColumnWidth::Percentage(60)),
            make_column(ColumnWidth::Percentage(50)),
        ];
        let widths = calculate_column_widths(&columns);
        assert!((widths[0] - 0.0).abs() < f64::EPSILON);
        let pct_sum: f64 = widths[1] + widths[2];
        assert!((pct_sum - 100.0).abs() < 0.01, "pct sum was {pct_sum}");
        assert!((widths[1] - 60.0 * 100.0 / 110.0).abs() < 0.01);
        assert!((widths[2] - 50.0 * 100.0 / 110.0).abs() < 0.01);
    }

    #[test]
    fn test_all_auto_no_normalization() {
        let columns = vec![
            make_column(ColumnWidth::Auto),
            make_column(ColumnWidth::Auto),
        ];
        let widths = calculate_column_widths(&columns);
        assert_eq!(widths, vec![0.0, 0.0]);
    }

    #[test]
    fn test_complex_proportional() {
        // [cols="2,1,1,1,1"] -> 33.33%, 16.67%, 16.67%, 16.67%, 16.67%
        let columns = vec![
            make_column(ColumnWidth::Proportional(2)),
            make_column(ColumnWidth::Proportional(1)),
            make_column(ColumnWidth::Proportional(1)),
            make_column(ColumnWidth::Proportional(1)),
            make_column(ColumnWidth::Proportional(1)),
        ];
        let widths = calculate_column_widths(&columns);
        assert_eq!(widths.len(), 5);
        // 2/(2+1+1+1+1) = 2/6 = 33.33%
        assert!((widths[0] - 33.333).abs() < 0.01);
        // 1/6 = 16.67%
        for w in &widths[1..] {
            assert!((*w - 16.667).abs() < 0.01);
        }
    }

    mod grid {
        use super::*;
        use acdc_parser::{Block, DelimitedBlockType};

        /// Parse an `AsciiDoc` string and extract the first table.
        ///
        /// Leaks the parsed document so the returned `Table<'static>` borrows
        /// from memory that lives for the rest of the test process.
        #[allow(clippy::expect_used)]
        fn parse_table(adoc: &str) -> Table<'static> {
            let options = acdc_parser::Options::default();
            let parsed = acdc_parser::parse(adoc, &options).expect("Failed to parse AsciiDoc");
            let parsed: &'static acdc_parser::ParsedDocument = Box::leak(Box::new(parsed));
            parsed
                .document()
                .blocks
                .iter()
                .find_map(|block| {
                    if let Block::DelimitedBlock(db) = block
                        && let DelimitedBlockType::DelimitedTable(table) = &db.inner
                    {
                        return Some(table.clone());
                    }
                    None
                })
                .expect("No table found in document")
        }

        #[test]
        fn test_colspan() {
            let table = parse_table(
                r#"[cols="3*"]
|===
| A | B | C

2+| Spans two columns | D
| E | F | G
|==="#,
            );

            assert_eq!(determine_column_count(&table), 3);
            assert!(table_has_spans(&table));

            let grid = build_grid(&table, 3);
            // Header + 2 body rows = 3
            assert_eq!(grid.len(), 3);

            // Header row: 3 content cells
            assert_eq!(grid[0].cells.len(), 3);
            assert!(grid[0].is_header);
            assert!(!grid[0].is_footer);

            // Row with colspan: Content, HSpan, Content
            assert_eq!(
                grid[1].cells,
                vec![
                    CellKind::Content { cell_index: 0 },
                    CellKind::HSpan,
                    CellKind::Content { cell_index: 1 },
                ]
            );

            // Normal row: 3 content cells
            assert_eq!(
                grid[2].cells,
                vec![
                    CellKind::Content { cell_index: 0 },
                    CellKind::Content { cell_index: 1 },
                    CellKind::Content { cell_index: 2 },
                ]
            );
        }

        #[test]
        fn test_rowspan() {
            let table = parse_table(
                r"|===
| A | B | C

.2+| Spans rows | D | E
| F | G
| H | I | J
|===",
            );

            assert_eq!(determine_column_count(&table), 3);
            assert!(table_has_spans(&table));

            let grid = build_grid(&table, 3);
            assert_eq!(grid.len(), 4);

            // Row with rowspan: Content(0), Content(1), Content(2)
            assert_eq!(
                grid[1].cells,
                vec![
                    CellKind::Content { cell_index: 0 },
                    CellKind::Content { cell_index: 1 },
                    CellKind::Content { cell_index: 2 },
                ]
            );

            // Next row: VSpan, Content(0), Content(1)
            assert_eq!(
                grid[2].cells,
                vec![
                    CellKind::VSpan,
                    CellKind::Content { cell_index: 0 },
                    CellKind::Content { cell_index: 1 },
                ]
            );
        }

        #[test]
        fn test_combined_span() {
            let table = parse_table(
                r"|===
| A | B | C | D

2.2+| Big cell | E | F
| G | H
| I | J | K | L
|===",
            );

            assert_eq!(determine_column_count(&table), 4);
            assert!(table_has_spans(&table));

            let grid = build_grid(&table, 4);
            assert_eq!(grid.len(), 4);

            // Row with 2.2+ span: Content(0), HSpan, Content(1), Content(2)
            assert_eq!(
                grid[1].cells,
                vec![
                    CellKind::Content { cell_index: 0 },
                    CellKind::HSpan,
                    CellKind::Content { cell_index: 1 },
                    CellKind::Content { cell_index: 2 },
                ]
            );

            // Next row: VSpan, VSpan, Content(0), Content(1)
            assert_eq!(
                grid[2].cells,
                vec![
                    CellKind::VSpan,
                    CellKind::VSpan,
                    CellKind::Content { cell_index: 0 },
                    CellKind::Content { cell_index: 1 },
                ]
            );
        }

        #[test]
        fn test_no_spans() {
            let table = parse_table(
                r"|===
| A | B
| C | D
|===",
            );

            assert!(!table_has_spans(&table));
        }

        #[test]
        fn test_footer_flag() {
            let table = parse_table(
                r"[%header%footer]
|===
| H1 | H2

| B1 | B2

| F1 | F2
|===",
            );

            let grid = build_grid(&table, 2);
            assert_eq!(grid.len(), 3);
            assert!(grid[0].is_header);
            assert!(!grid[0].is_footer);
            assert!(!grid[1].is_header);
            assert!(!grid[1].is_footer);
            assert!(!grid[2].is_header);
            assert!(grid[2].is_footer);
        }
    }
}
