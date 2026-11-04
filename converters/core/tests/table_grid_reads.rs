//! Typed table grids preserve span placement and access to original cells.

use acdc_converters_core::table::{
    CellKind::{Content, HSpan, VSpan},
    build_grid, column_count, determine_column_count, grid_rows, has_spans, table_has_spans,
};
use acdc_parser::{
    Block, ColumnFormat, CompatibilityMode, DelimitedBlock, DelimitedBlockType, Location, Options,
    Table, TableRow,
    ast::{BlockKind, BlockRef, TableRef},
    parse,
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn table_ref<'a>(block: &'a Block<'a>) -> TestResult<TableRef<'a>> {
    let BlockKind::Table(table) = BlockRef::from(block).kind() else {
        return Err("expected table".into());
    };
    Ok(table)
}

fn owner_table<'a>(block: &'a Block<'a>) -> TestResult<Table<'a>> {
    let Block::DelimitedBlock(block) = block else {
        return Err("expected delimited block".into());
    };
    let DelimitedBlockType::DelimitedTable(table) = block.clone_body() else {
        return Err("expected table".into());
    };
    Ok(table)
}

fn table_block(table: Table<'_>) -> Block<'_> {
    Block::DelimitedBlock(DelimitedBlock::new(
        DelimitedBlockType::DelimitedTable(table),
        "|===",
        Location::default(),
    ))
}

fn assert_legacy_matches(block: &Block<'_>, width: usize) -> TestResult {
    let table = table_ref(block)?;
    let owner = owner_table(block)?;
    assert_eq!(column_count(table), determine_column_count(&owner));
    assert_eq!(has_spans(table), table_has_spans(&owner));
    let typed = grid_rows(table, width);
    let legacy = build_grid(&owner, width);
    assert_eq!(typed.len(), legacy.len());
    for (typed, legacy) in typed.iter().zip(&legacy) {
        assert_eq!(typed.cells, legacy.cells);
        assert_eq!(typed.cells.len(), width);
        assert_eq!(typed.is_header, legacy.is_header);
        assert_eq!(typed.is_footer, legacy.is_footer);
        for marker in &typed.cells {
            if let Content { cell_index } = *marker {
                let cell = typed
                    .ast_row
                    .cells()
                    .nth(cell_index)
                    .ok_or("content cell")?;
                let original = legacy
                    .ast_row
                    .columns
                    .get(cell_index)
                    .ok_or("original cell")?;
                assert_eq!(cell.colspan(), original.colspan);
                assert_eq!(cell.rowspan(), original.rowspan);
                let blocks = cell.blocks().collect::<Vec<_>>();
                assert_eq!(blocks.len(), original.content.len());
                for (actual, expected) in blocks.into_iter().zip(&original.content) {
                    assert_eq!(
                        actual.location().absolute_start(),
                        expected.location().absolute_start
                    );
                    assert_eq!(
                        actual.location().absolute_end(),
                        expected.location().absolute_end
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn typed_grids_preserve_row_roles_combined_spans_and_source_cells() -> TestResult {
    let source = "\
[cols=\"4*\",options=\"header,footer\"]
|===
|H1 |H2 |H3 |H4

2.2+|Wide *café* |A |B
|C |D

|F1 |F2 |F3 |F4
|===
";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let parsed_blocks: Vec<_> = parsed
            .document()
            .blocks()
            .map(acdc_parser::Block::from)
            .collect();
        let block = parsed_blocks.first().ok_or("table block")?;
        assert_legacy_matches(block, 4)?;
        let grid = grid_rows(table_ref(block)?, 4);
        let [header, first, second, footer] = grid.as_slice() else {
            return Err("expected four rows".into());
        };
        assert!(header.is_header);
        assert!(!header.is_footer);
        assert!(!(first.is_header || first.is_footer));
        assert!(!(second.is_header || second.is_footer));
        assert!(!footer.is_header);
        assert!(footer.is_footer);
        assert_eq!(
            first.cells,
            [
                Content { cell_index: 0 },
                HSpan,
                Content { cell_index: 1 },
                Content { cell_index: 2 }
            ]
        );
        assert_eq!(
            second.cells,
            [
                VSpan,
                VSpan,
                Content { cell_index: 0 },
                Content { cell_index: 1 }
            ]
        );
        let wide = first.ast_row.cells().next().ok_or("spanning cell")?;
        assert_eq!((wide.colspan(), wide.rowspan()), (2, 2));
        let content = wide.blocks().next().ok_or("cell paragraph")?;
        assert!(
            source
                .get(content.location().absolute_start()..content.location().absolute_end())
                .ok_or("cell source")?
                .contains("café")
        );
    }
    Ok(())
}

#[test]
fn typed_grids_preserve_empty_rows_and_column_fallbacks() -> TestResult {
    let empty = table_block(Table::new(Vec::new(), Location::default()));
    assert_eq!(column_count(table_ref(&empty)?), 1);
    assert!(!has_spans(table_ref(&empty)?));
    assert!(grid_rows(table_ref(&empty)?, 1).is_empty());
    assert_legacy_matches(&empty, 1)?;

    let rows = table_block(
        Table::new(vec![TableRow::new(Vec::new())], Location::default())
            .with_header(Some(TableRow::new(Vec::new())))
            .with_footer(Some(TableRow::new(Vec::new()))),
    );
    assert_eq!(column_count(table_ref(&rows)?), 0);
    for width in [0, 1, 3] {
        assert_legacy_matches(&rows, width)?;
        let grid = grid_rows(table_ref(&rows)?, width);
        assert_eq!(grid.len(), 3);
        for row in grid {
            assert_eq!(row.cells, vec![HSpan; width]);
            assert_eq!(row.ast_row.cells().count(), 0);
        }
    }
    Ok(())
}

#[test]
fn typed_grids_preserve_truncation_padding_and_constructed_spans() -> TestResult {
    let parsed = parse("|===\n|Seed\n|===\n", &Options::default())?;
    let parsed_blocks: Vec<_> = parsed
        .document()
        .blocks()
        .map(acdc_parser::Block::from)
        .collect();
    let seed_block = parsed_blocks.first().ok_or("seed table")?;
    let seed_table = owner_table(seed_block)?;
    let seed = seed_table
        .rows
        .first()
        .and_then(|row| row.columns.first())
        .ok_or("seed cell")?;
    let mut zero = seed.clone();
    zero.colspan = 0;
    zero.rowspan = 0;
    let mut wide = seed.clone();
    wide.colspan = 7;
    wide.rowspan = 3;
    let block = table_block(
        Table::new(
            vec![
                TableRow::new(vec![zero, wide, seed.clone()]),
                TableRow::new(Vec::new()),
            ],
            Location::default(),
        )
        .with_footer(Some(TableRow::new(vec![seed.clone()])))
        .with_columns(vec![ColumnFormat::new(); 3]),
    );
    assert_eq!(column_count(table_ref(&block)?), 3);
    assert!(has_spans(table_ref(&block)?));
    let inferred_spans = table_block(owner_table(&block)?.with_columns(Vec::new()));
    assert_eq!(column_count(table_ref(&inferred_spans)?), 9);
    assert_legacy_matches(&inferred_spans, 9)?;
    for width in [0, 1, 2, 3, 8, 10] {
        assert_legacy_matches(&block, width)?;
    }
    let grid = grid_rows(table_ref(&block)?, 3);
    let [first, second, footer] = grid.as_slice() else {
        return Err("expected three rows".into());
    };
    assert_eq!(
        first.cells,
        [Content { cell_index: 0 }, Content { cell_index: 1 }, HSpan]
    );
    assert_eq!(second.cells, [HSpan, VSpan, VSpan]);
    assert_eq!(footer.cells, [Content { cell_index: 0 }, VSpan, VSpan]);
    assert!(footer.is_footer);

    let inferred = table_block(Table::new(
        vec![
            TableRow::new(vec![seed.clone(); 2]),
            TableRow::new(vec![seed.clone(); 4]),
        ],
        Location::default(),
    ));
    assert_eq!(column_count(table_ref(&inferred)?), 4);
    assert_legacy_matches(&inferred, 4)?;
    assert_eq!(
        grid_rows(table_ref(&inferred)?, 4)
            .first()
            .ok_or("inferred row")?
            .cells,
        [
            Content { cell_index: 0 },
            Content { cell_index: 1 },
            HSpan,
            HSpan
        ]
    );
    Ok(())
}
