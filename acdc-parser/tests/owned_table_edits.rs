//! Prepared table edits preserve row groups, source lifetimes and cell snapshots.

use std::{alloc::System, hint::black_box};

use acdc_parser::{
    Block, ColumnFormat, ColumnStyle, ColumnWidth, CompatibilityMode, DelimitedBlock,
    DelimitedBlockType, HorizontalAlignment, Location, Options, Paragraph, ParseResult, Table,
    TableColumn, TableFrame, TableGrid, TableRow, VerticalAlignment,
    ast::{BlockKind, BlockRef, ColumnFormatRef, TableCellRef, TableRef, TableRowRef},
    parse,
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

type Error = Box<dyn std::error::Error>;

const SOURCE: &str = "= Table\n:scope-value: retained\n\n[cols=\"a,>s\",options=\"header,footer\",frame=none,grid=none]\n|===\n|Head one |Head two\n\n|Body footnote:[Note]. |Strong body\n\n|Foot one |Foot two\n|===\n";

fn table(parsed: &ParseResult) -> Result<TableRef<'_>, Error> {
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
        .ok_or_else(|| "table".into())
}

fn block(table: Table<'_>) -> DelimitedBlock<'_> {
    DelimitedBlock::new(
        DelimitedBlockType::DelimitedTable(table),
        "|===",
        Location::default(),
    )
}

fn read<'a>(block: &'a DelimitedBlock<'a>) -> Result<TableRef<'a>, Error> {
    let BlockKind::Table(table) = BlockRef::from(block).kind() else {
        return Err("table".into());
    };
    Ok(table)
}

#[rstest::rstest]
fn selected_copies_preserve_rows_formats_presentation_and_initial_scopes(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let original_document = serde_json::to_value(parsed.document())?;
    let original = table(&parsed)?;
    let copied = Table::from(original);
    let wrapper = block(copied.clone());
    let copied_read = read(&wrapper)?;
    assert_eq!(Table::from(copied_read), copied);
    assert_eq!(copied_read.rows().count(), original.rows().count());
    assert_eq!(copied_read.columns().count(), original.columns().count());
    assert_eq!(copied_read.presentation(), original.presentation());
    assert_eq!(
        Location::from(copied_read.delimited_location()),
        Location::from(original.delimited_location())
    );
    assert!(original.location().absolute_start() < original.delimited_location().absolute_start());
    assert_eq!(
        copied_read.presentation().ok_or("presentation")?.frame(),
        TableFrame::None
    );
    assert_eq!(
        copied_read.presentation().ok_or("presentation")?.grid(),
        TableGrid::None
    );
    for (actual, expected) in copied_read.rows().zip(original.rows()) {
        let row = TableRow::from(actual);
        assert_eq!(row, TableRow::from(expected));
        for (actual, expected) in actual.cells().zip(expected.cells()) {
            assert_eq!(TableColumn::from(actual), TableColumn::from(expected));
            assert_eq!(
                actual.initial_attributes().collect::<Vec<_>>(),
                expected.initial_attributes().collect::<Vec<_>>()
            );
        }
    }
    for (actual, expected) in copied_read.columns().zip(original.columns()) {
        assert_eq!(ColumnFormat::from(actual), ColumnFormat::from(expected));
    }
    assert_eq!(serde_json::to_value(parsed.document())?, original_document);
    Ok(())
}

#[rstest::rstest]
fn table_parts_outlive_the_previous_owner_and_round_trip_without_layout_repairs(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let original = Table::from(table(&parsed)?);
    let (header, footer, rows, columns) = {
        let mut local = original.clone();
        let result = (
            local.take_header(),
            local.take_footer(),
            local.take_rows(),
            local.take_columns(),
        );
        let empty = block(local);
        let empty = read(&empty)?;
        assert!(empty.header().is_none());
        assert!(empty.footer().is_none());
        assert_eq!(empty.rows().count(), 0);
        assert_eq!(empty.columns().count(), 0);
        result
    };
    let mut rebuilt = original.clone();
    rebuilt.set_header(header);
    rebuilt.set_footer(footer);
    rebuilt.set_rows(rows);
    rebuilt.set_columns(columns);
    assert_eq!(rebuilt, original);
    assert_eq!(
        serde_json::to_value(&rebuilt)?,
        serde_json::to_value(&original)?
    );
    rebuilt.set_location(Location::default());
    rebuilt.push_row(TableRow::new(Vec::new()));
    let wrapper = block(rebuilt);
    assert_eq!(
        read(&wrapper)?.rows().count(),
        table(&parsed)?.rows().count() + 1
    );
    assert_eq!(
        read(&wrapper)?.presentation(),
        table(&parsed)?.presentation()
    );
    assert_eq!(
        Location::from(read(&wrapper)?.delimited_location()),
        Location::default()
    );
    Ok(())
}

#[test]
fn present_empty_header_and_footer_are_independent_from_body_rows() -> Result<(), Error> {
    let mut table = Table::new(Vec::new(), Location::default());
    table.set_header(Some(TableRow::new(Vec::new())));
    table.set_footer(Some(TableRow::new(Vec::new())));
    let wrapper = block(table.clone());
    let table_read = read(&wrapper)?;
    assert_eq!(table_read.header().ok_or("header")?.cells().count(), 0);
    assert_eq!(table_read.footer().ok_or("footer")?.cells().count(), 0);
    assert_eq!(table_read.rows().count(), 0);
    assert!(table.take_header().is_some());
    assert!(table.take_footer().is_some());
    let wrapper = block(table);
    assert!(read(&wrapper)?.header().is_none());
    assert!(read(&wrapper)?.footer().is_none());
    assert_eq!(read(&wrapper)?.rows().count(), 0);
    assert!(read(&wrapper)?.presentation().is_none());
    Ok(())
}

#[rstest::rstest]
fn row_cell_transfers_outlive_the_previous_row(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let original = table(&parsed)?.rows().next().ok_or("row")?;
    let cells = {
        let mut local = TableRow::from(original);
        local.take_cells()
    };
    let mut rebuilt = TableRow::new(Vec::new());
    rebuilt.set_cells(cells);
    assert_eq!(rebuilt, TableRow::from(original));
    rebuilt.push_cell(TableColumn::new(Vec::new()));
    assert_eq!(
        TableRowRef::from(&rebuilt).cells().count(),
        original.cells().count() + 1
    );
    Ok(())
}

#[rstest::rstest]
fn cell_content_transfers_keep_the_initial_scope_and_formatting(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let original_document = serde_json::to_value(parsed.document())?;
    let original = table(&parsed)?
        .rows()
        .next()
        .ok_or("row")?
        .cells()
        .next()
        .ok_or("cell")?;
    assert!(original.initial_attributes().next().is_some());
    let blocks = {
        let mut local = TableColumn::from(original);
        let blocks = local.take_blocks();
        assert_eq!(TableCellRef::from(&local).blocks().count(), 0);
        assert_eq!(
            TableCellRef::from(&local)
                .initial_attributes()
                .collect::<Vec<_>>(),
            original.initial_attributes().collect::<Vec<_>>()
        );
        blocks
    };
    let mut rebuilt = TableColumn::from(original);
    rebuilt.set_blocks(blocks);
    assert_eq!(rebuilt, TableColumn::from(original));
    rebuilt.push_block(Block::Paragraph(Paragraph::new(
        Vec::new(),
        Location::default(),
    )));
    assert_eq!(
        TableCellRef::from(&rebuilt).blocks().count(),
        original.blocks().count() + 1
    );
    assert_eq!(
        TableCellRef::from(&rebuilt)
            .initial_attributes()
            .collect::<Vec<_>>(),
        original.initial_attributes().collect::<Vec<_>>()
    );
    assert_eq!(serde_json::to_value(parsed.document())?, original_document);
    Ok(())
}

#[test]
fn constructed_cells_keep_explicit_spans_and_optional_format_overrides() {
    let mut cell = TableColumn::new(Vec::new());
    cell.set_colspan(0);
    cell.set_rowspan(3);
    cell.set_halign(Some(HorizontalAlignment::Center));
    cell.set_valign(Some(VerticalAlignment::Bottom));
    cell.set_style(Some(ColumnStyle::Strong));
    let read = TableCellRef::from(&cell);
    assert_eq!(read.colspan(), 0);
    assert_eq!(read.rowspan(), 3);
    assert_eq!(read.halign(), Some(HorizontalAlignment::Center));
    assert_eq!(read.valign(), Some(VerticalAlignment::Bottom));
    assert_eq!(read.style(), Some(ColumnStyle::Strong));
    cell.set_colspan(1);
    cell.set_rowspan(1);
    cell.set_halign(None);
    cell.set_valign(None);
    cell.set_style(None);
    let read = TableCellRef::from(&cell);
    assert_eq!(read.colspan(), 1);
    assert_eq!(read.rowspan(), 1);
    assert!(read.halign().is_none());
    assert!(read.valign().is_none());
    assert!(read.style().is_none());
}

#[test]
fn column_format_copies_preserve_all_prepared_values_without_allocating() {
    let original = ColumnFormat::new()
        .with_halign(HorizontalAlignment::Right)
        .with_valign(VerticalAlignment::Middle)
        .with_width(ColumnWidth::Percentage(25))
        .with_style(ColumnStyle::Emphasis);
    let read = ColumnFormatRef::from(&original);
    let region = Region::new(ALLOCATOR);
    let copy = black_box(ColumnFormat::from(read));
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(copy, original);
}

#[test]
fn selected_row_and_cell_copy_cost_does_not_grow_with_unrelated_table_size() -> Result<(), Error> {
    fn cost<T>(copy: impl FnOnce() -> T) -> (usize, usize) {
        let region = Region::new(ALLOCATOR);
        let value = black_box(copy());
        let stats = region.change();
        drop(value);
        (stats.allocations, stats.bytes_allocated)
    }
    let mut rows = Vec::new();
    let mut cells = Vec::new();
    for count in [1, 1000] {
        let source = format!("|===\n{}|===\n", "|Same\n".repeat(count));
        let parsed = parse(&source, &Options::default())?;
        assert_eq!(table(&parsed)?.rows().count(), count);
        let row = table(&parsed)?.rows().next().ok_or("row")?;
        assert_eq!(row.cells().count(), 1);
        rows.push(cost(|| TableRow::from(row)));
        let cell = row.cells().next().ok_or("cell")?;
        cells.push(cost(|| TableColumn::from(cell)));
    }
    assert_eq!(rows.first(), rows.last());
    assert_eq!(cells.first(), cells.last());
    let mut wide_cells = Vec::new();
    // Column counts are capped at 100; row counts vary independently above.
    for count in [1, 100] {
        let source = format!(
            "[cols=\"{count}*\"]\n|===\n{}\n|===\n",
            "|Same".repeat(count)
        );
        let parsed = parse(&source, &Options::default())?;
        let row = table(&parsed)?.rows().next().ok_or("row")?;
        assert_eq!(row.cells().count(), count);
        let cell = row.cells().next().ok_or("cell")?;
        wide_cells.push(cost(|| TableColumn::from(cell)));
    }
    assert_eq!(wide_cells.first(), wide_cells.last());
    Ok(())
}
