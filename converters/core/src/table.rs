//! Table utilities shared between converters.
//!
//! The grid utilities ([`grid_rows`], [`CellKind`], [`GridRow`]) reconstruct
//! logical cell positions for converters that need explicit placement or span
//! placeholders (e.g. PDF, manpage tbl, terminal). Formats whose row containers
//! provide placement directly (like HTML `<tr>`) can iterate the AST cells
//! without building a grid.

use acdc_parser::{
    ColumnWidth,
    ast::{ColumnFormatRef, TableRef, TableRowRef},
};

/// Calculate percentage widths from typed column formats.
///
/// Numeric widths share one scale. With automatic columns, numeric widths below
/// a combined 100% keep their authored values; otherwise they fill 100%.
/// `None` denotes an automatic column, distinct from an explicit zero width.
/// All-zero numeric columns receive equal widths. Results are finite percentages
/// between zero and 100. An empty iterator produces no widths.
#[must_use]
pub fn column_widths<'column>(
    columns: impl Iterator<Item = ColumnFormatRef<'column>>,
) -> Vec<Option<f64>> {
    let mut widths: Vec<_> = columns
        .map(|c| match c.width() {
            ColumnWidth::Proportional(width) | ColumnWidth::Percentage(width) => {
                Some(f64::from(width))
            }
            ColumnWidth::Auto | _ => None,
        })
        .collect();
    // Convert before summing: even a vector of maximum u32 widths has a finite
    // f64 sum, while summing the authored integers can overflow.
    let sum: f64 = widths.iter().flatten().sum();
    let automatic = widths.iter().any(Option::is_none);
    if sum == 0.0 && !automatic && !widths.is_empty() {
        let count: f64 = widths.iter().map(|_| 1.0).sum();
        widths.fill(Some(100.0 / count));
    } else if sum > 0.0 && (sum - 100.0).abs() > f64::EPSILON && (!automatic || sum > 100.0) {
        for width in widths.iter_mut().flatten() {
            *width = *width / sum * 100.0;
        }
    }

    widths
}

/// What occupies a logical cell position in a table grid.
///
/// Used by converters that need explicit positions or placeholders to build a
/// normalized grid where every row has the same number of columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellKind {
    /// A real cell with content, indexed in the original row's cell order.
    Content {
        /// Index into the original row's cells, excluding span placeholders.
        cell_index: usize,
    },
    /// Horizontal span placeholder, also used to pad missing cells.
    HSpan,
    /// Vertical span placeholder (the primary cell is above).
    VSpan,
}

/// A logical row in the grid, with metadata about its role.
#[derive(Debug)]
pub struct GridRow<Row> {
    /// The cell kinds for each logical column position.
    pub cells: Vec<CellKind>,
    /// The original row, retained as a reference or typed handle.
    pub ast_row: Row,
    /// Whether this row is a header row.
    pub is_header: bool,
    /// Whether this row is a footer row.
    pub is_footer: bool,
}

/// Count logical columns, using explicit column formats when present.
///
/// Otherwise, use the widest row after accounting for colspans. A table without
/// rows has one column; a table containing only empty rows has zero.
#[must_use]
pub fn column_count(table: TableRef<'_>) -> usize {
    count_columns(table.columns().count(), table_rows(table))
}

fn count_columns<'a>(columns: usize, rows: impl Iterator<Item = TableRowRef<'a>>) -> usize {
    if columns > 0 {
        return columns;
    }
    rows.map(|row| row.cells().map(|cell| cell.colspan().max(1)).sum::<usize>())
        .max()
        .unwrap_or(1)
}

fn table_rows(table: TableRef<'_>) -> impl Iterator<Item = TableRowRef<'_>> {
    table
        .header()
        .into_iter()
        .chain(table.rows())
        .chain(table.footer())
}

/// Build a rectangular grid while retaining typed handles to its original rows.
///
/// Rows remain in header, body, footer order. Content indices address
/// [`TableRowRef::cells`]; placeholders represent covered or missing positions.
/// Spans are clamped to the supplied width. Zero width retains the rows with
/// empty cell vectors.
#[must_use]
pub fn grid_rows(table: TableRef<'_>, num_cols: usize) -> Vec<GridRow<TableRowRef<'_>>> {
    let rows = table
        .header()
        .into_iter()
        .map(|row| (row, true, false))
        .chain(table.rows().map(|row| (row, false, false)))
        .chain(table.footer().map(|row| (row, false, true)));
    build_grid_rows(rows, num_cols, |row| {
        row.cells().map(|cell| (cell.colspan(), cell.rowspan()))
    })
}

fn build_grid_rows<Row: Copy, Cells: Iterator<Item = (usize, usize)>>(
    rows: impl Iterator<Item = (Row, bool, bool)>,
    num_cols: usize,
    cell_spans: impl Fn(Row) -> Cells,
) -> Vec<GridRow<Row>> {
    let mut grid = Vec::with_capacity(rows.size_hint().0);
    let mut rowspan_remaining = vec![0usize; num_cols];

    for (ast_row, is_header, is_footer) in rows {
        let mut cells = cell_spans(ast_row).enumerate();
        let mut row_cells = Vec::with_capacity(num_cols);
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

            let Some((cell_index, (colspan, rowspan))) = cells.next() else {
                // Shouldn't happen in well-formed input; fill remaining
                row_cells.push(CellKind::HSpan);
                col += 1;
                continue;
            };

            let colspan = colspan.max(1);
            let rowspan = rowspan.max(1);

            row_cells.push(CellKind::Content { cell_index });

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
        }

        grid.push(GridRow {
            cells: row_cells,
            ast_row,
            is_header,
            is_footer,
        });
    }

    grid
}

/// Whether any header, body, or footer cell spans multiple rows or columns.
#[must_use]
pub fn has_spans(table: TableRef<'_>) -> bool {
    rows_have_spans(table_rows(table))
}

fn rows_have_spans<'a>(mut rows: impl Iterator<Item = TableRowRef<'a>>) -> bool {
    rows.any(|row| {
        row.cells()
            .any(|cell| cell.colspan() > 1 || cell.rowspan() > 1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::ColumnFormat;

    fn make_column(width: ColumnWidth) -> ColumnFormat {
        ColumnFormat::new().with_width(width)
    }

    fn assert_widths_close(actual: &[Option<f64>], expected: &[f64]) {
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert!(actual.is_some_and(|actual| (actual - *expected).abs() < 0.01));
        }
    }

    #[test]
    fn test_proportional_widths() {
        let columns = [
            make_column(ColumnWidth::Proportional(1)),
            make_column(ColumnWidth::Proportional(3)),
        ];
        let widths = column_widths(columns.iter().map(ColumnFormatRef::from));
        assert_widths_close(&widths, &[25.0, 75.0]);
    }

    #[test]
    fn test_equal_proportional_widths() {
        let columns = [
            make_column(ColumnWidth::Proportional(1)),
            make_column(ColumnWidth::Proportional(1)),
            make_column(ColumnWidth::Proportional(1)),
        ];
        let widths = column_widths(columns.iter().map(ColumnFormatRef::from));
        assert_eq!(widths.len(), 3);
        for w in &widths {
            assert!(w.is_some_and(|w| (w - 33.333).abs() < 0.01));
        }
    }

    #[test]
    fn test_percentage_widths() {
        let columns = [
            make_column(ColumnWidth::Percentage(25)),
            make_column(ColumnWidth::Percentage(75)),
        ];
        let widths = column_widths(columns.iter().map(ColumnFormatRef::from));
        assert_eq!(widths, vec![Some(25.0), Some(75.0)]);
    }

    #[test]
    fn test_auto_widths() {
        let columns = [
            make_column(ColumnWidth::Auto),
            make_column(ColumnWidth::Auto),
        ];
        let widths = column_widths(columns.iter().map(ColumnFormatRef::from));
        assert_eq!(widths, vec![None, None]);
    }

    #[test]
    fn test_empty_columns() {
        let widths = column_widths(std::iter::empty());
        assert_eq!(widths, [] as [Option<f64>; 0]);
    }

    #[test]
    fn test_percentage_over_100_normalized() {
        // [cols="34%,36%,31%"] sums to 101% → normalize to 100%
        let columns = [
            make_column(ColumnWidth::Percentage(34)),
            make_column(ColumnWidth::Percentage(36)),
            make_column(ColumnWidth::Percentage(31)),
        ];
        let widths = column_widths(columns.iter().map(ColumnFormatRef::from));
        let sum: f64 = widths.iter().flatten().sum();
        assert!((sum - 100.0).abs() < 0.01, "sum was {sum}");
        assert_widths_close(
            &widths,
            &[
                34.0 * 100.0 / 101.0,
                36.0 * 100.0 / 101.0,
                31.0 * 100.0 / 101.0,
            ],
        );
    }

    #[test]
    fn test_percentage_under_100_normalized() {
        // [cols="20%,30%,40%"] sums to 90% → normalize to 100%
        let columns = [
            make_column(ColumnWidth::Percentage(20)),
            make_column(ColumnWidth::Percentage(30)),
            make_column(ColumnWidth::Percentage(40)),
        ];
        let widths = column_widths(columns.iter().map(ColumnFormatRef::from));
        let sum: f64 = widths.iter().flatten().sum();
        assert!((sum - 100.0).abs() < 0.01, "sum was {sum}");
        assert_widths_close(
            &widths,
            &[
                20.0 * 100.0 / 90.0,
                30.0 * 100.0 / 90.0,
                40.0 * 100.0 / 90.0,
            ],
        );
    }

    #[test]
    fn test_percentage_exact_100_unchanged() {
        let columns = [
            make_column(ColumnWidth::Percentage(25)),
            make_column(ColumnWidth::Percentage(25)),
            make_column(ColumnWidth::Percentage(50)),
        ];
        let widths = column_widths(columns.iter().map(ColumnFormatRef::from));
        assert_eq!(widths, vec![Some(25.0), Some(25.0), Some(50.0)]);
    }

    #[test]
    fn test_auto_with_percentage_over_100() {
        // [cols="~,60%,50%"] non-auto sum=110% → normalize percentages, auto stays 0.0
        let columns = [
            make_column(ColumnWidth::Auto),
            make_column(ColumnWidth::Percentage(60)),
            make_column(ColumnWidth::Percentage(50)),
        ];
        let widths = column_widths(columns.iter().map(ColumnFormatRef::from));
        let pct_sum: f64 = widths.iter().flatten().sum();
        assert!((pct_sum - 100.0).abs() < 0.01, "pct sum was {pct_sum}");
        let mut widths = widths.into_iter();
        assert_eq!(widths.next(), Some(None));
        assert_widths_close(
            &widths.collect::<Vec<_>>(),
            &[60.0 * 100.0 / 110.0, 50.0 * 100.0 / 110.0],
        );
    }

    #[test]
    fn test_all_auto_no_normalization() {
        let columns = [
            make_column(ColumnWidth::Auto),
            make_column(ColumnWidth::Auto),
        ];
        let widths = column_widths(columns.iter().map(ColumnFormatRef::from));
        assert_eq!(widths, vec![None, None]);
    }

    #[test]
    fn test_complex_proportional() {
        // [cols="2,1,1,1,1"] -> 33.33%, 16.67%, 16.67%, 16.67%, 16.67%
        let columns = [
            make_column(ColumnWidth::Proportional(2)),
            make_column(ColumnWidth::Proportional(1)),
            make_column(ColumnWidth::Proportional(1)),
            make_column(ColumnWidth::Proportional(1)),
            make_column(ColumnWidth::Proportional(1)),
        ];
        let widths = column_widths(columns.iter().map(ColumnFormatRef::from));
        assert_widths_close(&widths, &[33.333, 16.667, 16.667, 16.667, 16.667]);
    }

    mod grid {
        use super::*;
        use acdc_parser::{Options, ParseResult, ast::BlockKind};

        fn parse_table(adoc: &str) -> Result<ParseResult, Box<dyn std::error::Error>> {
            Ok(acdc_parser::parse(adoc, &Options::default())?)
        }

        fn first_table(parsed: &ParseResult) -> Result<TableRef<'_>, Box<dyn std::error::Error>> {
            parsed
                .document()
                .blocks()
                .find_map(|block| {
                    if let BlockKind::Table(table) = block.kind() {
                        Some(table)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| std::io::Error::other("no table found in document").into())
        }

        fn assert_grid_rows(
            grid: &[GridRow<TableRowRef<'_>>],
            expected: &[(bool, bool, Vec<CellKind>)],
        ) {
            assert_eq!(grid.len(), expected.len());
            for (row, (is_header, is_footer, cells)) in grid.iter().zip(expected) {
                assert_eq!(row.is_header, *is_header);
                assert_eq!(row.is_footer, *is_footer);
                assert_eq!(&row.cells, cells);
            }
        }

        #[test]
        fn test_colspan() -> Result<(), Box<dyn std::error::Error>> {
            let parsed = parse_table(
                r#"[cols="3*"]
|===
| A | B | C

2+| Spans two columns | D
| E | F | G
|==="#,
            )?;
            let table = first_table(&parsed)?;

            assert_eq!(column_count(table), 3);
            assert!(has_spans(table));

            let grid = grid_rows(table, 3);
            assert_grid_rows(
                &grid,
                &[
                    (
                        true,
                        false,
                        vec![
                            CellKind::Content { cell_index: 0 },
                            CellKind::Content { cell_index: 1 },
                            CellKind::Content { cell_index: 2 },
                        ],
                    ),
                    (
                        false,
                        false,
                        vec![
                            CellKind::Content { cell_index: 0 },
                            CellKind::HSpan,
                            CellKind::Content { cell_index: 1 },
                        ],
                    ),
                    (
                        false,
                        false,
                        vec![
                            CellKind::Content { cell_index: 0 },
                            CellKind::Content { cell_index: 1 },
                            CellKind::Content { cell_index: 2 },
                        ],
                    ),
                ],
            );
            Ok(())
        }

        #[test]
        fn test_rowspan() -> Result<(), Box<dyn std::error::Error>> {
            let parsed = parse_table(
                r"|===
| A | B | C

.2+| Spans rows | D | E
| F | G
| H | I | J
|===",
            )?;
            let table = first_table(&parsed)?;

            assert_eq!(column_count(table), 3);
            assert!(has_spans(table));

            let grid = grid_rows(table, 3);
            assert_grid_rows(
                &grid,
                &[
                    (
                        true,
                        false,
                        vec![
                            CellKind::Content { cell_index: 0 },
                            CellKind::Content { cell_index: 1 },
                            CellKind::Content { cell_index: 2 },
                        ],
                    ),
                    (
                        false,
                        false,
                        vec![
                            CellKind::Content { cell_index: 0 },
                            CellKind::Content { cell_index: 1 },
                            CellKind::Content { cell_index: 2 },
                        ],
                    ),
                    (
                        false,
                        false,
                        vec![
                            CellKind::VSpan,
                            CellKind::Content { cell_index: 0 },
                            CellKind::Content { cell_index: 1 },
                        ],
                    ),
                    (
                        false,
                        false,
                        vec![
                            CellKind::Content { cell_index: 0 },
                            CellKind::Content { cell_index: 1 },
                            CellKind::Content { cell_index: 2 },
                        ],
                    ),
                ],
            );
            Ok(())
        }

        #[test]
        fn test_combined_span() -> Result<(), Box<dyn std::error::Error>> {
            let parsed = parse_table(
                r"|===
| A | B | C | D

2.2+| Big cell | E | F
| G | H
| I | J | K | L
|===",
            )?;
            let table = first_table(&parsed)?;

            assert_eq!(column_count(table), 4);
            assert!(has_spans(table));

            let grid = grid_rows(table, 4);
            assert_grid_rows(
                &grid,
                &[
                    (
                        true,
                        false,
                        vec![
                            CellKind::Content { cell_index: 0 },
                            CellKind::Content { cell_index: 1 },
                            CellKind::Content { cell_index: 2 },
                            CellKind::Content { cell_index: 3 },
                        ],
                    ),
                    (
                        false,
                        false,
                        vec![
                            CellKind::Content { cell_index: 0 },
                            CellKind::HSpan,
                            CellKind::Content { cell_index: 1 },
                            CellKind::Content { cell_index: 2 },
                        ],
                    ),
                    (
                        false,
                        false,
                        vec![
                            CellKind::VSpan,
                            CellKind::VSpan,
                            CellKind::Content { cell_index: 0 },
                            CellKind::Content { cell_index: 1 },
                        ],
                    ),
                    (
                        false,
                        false,
                        vec![
                            CellKind::Content { cell_index: 0 },
                            CellKind::Content { cell_index: 1 },
                            CellKind::Content { cell_index: 2 },
                            CellKind::Content { cell_index: 3 },
                        ],
                    ),
                ],
            );
            Ok(())
        }

        #[test]
        fn test_no_spans() -> Result<(), Box<dyn std::error::Error>> {
            let parsed = parse_table(
                r"|===
| A | B
| C | D
|===",
            )?;
            let table = first_table(&parsed)?;

            assert!(!has_spans(table));
            Ok(())
        }

        #[test]
        fn test_footer_flag() -> Result<(), Box<dyn std::error::Error>> {
            let parsed = parse_table(
                r"[%header%footer]
|===
| H1 | H2

| B1 | B2

| F1 | F2
|===",
            )?;
            let table = first_table(&parsed)?;

            let grid = grid_rows(table, 2);
            assert_grid_rows(
                &grid,
                &[
                    (
                        true,
                        false,
                        vec![
                            CellKind::Content { cell_index: 0 },
                            CellKind::Content { cell_index: 1 },
                        ],
                    ),
                    (
                        false,
                        false,
                        vec![
                            CellKind::Content { cell_index: 0 },
                            CellKind::Content { cell_index: 1 },
                        ],
                    ),
                    (
                        false,
                        true,
                        vec![
                            CellKind::Content { cell_index: 0 },
                            CellKind::Content { cell_index: 1 },
                        ],
                    ),
                ],
            );
            Ok(())
        }
    }
}
