use acdc_parser::{
    Options, TableFrame, TableGrid, TableStripes,
    ast::{BlockKind, DocumentRef, TableRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn tables(document: DocumentRef<'_>) -> Vec<TableRef<'_>> {
    document
        .blocks()
        .filter_map(|block| {
            if let BlockKind::Table(table) = block.kind() {
                Some(table)
            } else {
                None
            }
        })
        .collect()
}

type Presentation = (TableFrame, TableGrid, TableStripes);

fn assert_presentations(tables: &[TableRef<'_>], expected: &[Presentation]) {
    let actual = tables
        .iter()
        .map(|table| {
            table
                .presentation()
                .map(|value| (value.frame(), value.grid(), value.stripes()))
        })
        .collect::<Vec<_>>();
    let expected = expected.iter().copied().map(Some).collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn table_presentation_uses_local_values_and_reference_fallbacks() -> Result<(), Error> {
    let parsed = parse(
        r"[frame=ends,grid=rows,stripes=odd]
|===
|one
|===

[frame=topbot,grid=cols,stripes=even]
|===
|two
|===

[frame=INVALID,grid=INVALID,stripes=INVALID]
|===
|three
|===

[stripes=hover]
|===
|hover
|===

|===
|four
|===
",
        &Options::default(),
    )?;

    assert_presentations(
        &tables(parsed.document()),
        &[
            (TableFrame::Ends, TableGrid::Rows, TableStripes::Odd),
            (TableFrame::Ends, TableGrid::Columns, TableStripes::Even),
            (TableFrame::None, TableGrid::None, TableStripes::None),
            (TableFrame::All, TableGrid::All, TableStripes::Hover),
            (TableFrame::All, TableGrid::All, TableStripes::None),
        ],
    );
    Ok(())
}

#[test]
fn table_presentation_follows_document_attributes_in_source_order() -> Result<(), Error> {
    let parsed = parse(
        r"= Tables
:table-frame: sides
:table-grid: rows
:table-stripes: odd

|===
|one
|===

:table-frame: ends
:table-grid: cols
:table-stripes: hover

|===
|two
|===

:table-frame!:
:table-grid!:
:table-stripes!:

|===
|three
|===
",
        &Options::default(),
    )?;

    assert_presentations(
        &tables(parsed.document()),
        &[
            (TableFrame::Sides, TableGrid::Rows, TableStripes::Odd),
            (TableFrame::Ends, TableGrid::Columns, TableStripes::Hover),
            (TableFrame::All, TableGrid::All, TableStripes::None),
        ],
    );
    Ok(())
}
