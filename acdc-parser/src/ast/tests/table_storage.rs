use crate::{
    CompatibilityMode, DelimitedBlock, DelimitedBlockType, DocumentAttributes, Location, Options,
    Table, TableFrame, TableGrid, TablePresentation, TableStripes,
    ast::{BlockKind, BlockRef},
    parse,
};

type Error = Box<dyn std::error::Error>;
type Decoration = (TableFrame, TableGrid, TableStripes);

fn decoration(value: TablePresentation) -> Decoration {
    (value.frame(), value.grid(), value.stripes())
}

fn resolve(block: &DelimitedBlock<'_>, defaults: &DocumentAttributes<'_>) -> TablePresentation {
    let expected = TablePresentation::from_attributes(&block.metadata, |name| defaults.get(name));
    let actual = TablePresentation::resolve(BlockRef::from(block).attributes(), defaults.into());
    assert_eq!(actual, expected);
    assert_eq!(
        TablePresentation::resolve_with(BlockRef::from(block).attributes(), |name| defaults
            .get(name)),
        expected
    );
    actual
}

fn empty_table() -> DelimitedBlock<'static> {
    DelimitedBlock::new(
        DelimitedBlockType::DelimitedTable(Table::new(Vec::new(), Location::default())),
        "|===",
        Location::default(),
    )
}

#[rstest::rstest]
fn explicit_resolution_preserves_stored_source_position_and_unresolved_tables(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Tables\n:table-frame: sides\n:table-grid: rows\n:table-stripes: odd\n\n|===\n|One\n|===\n\n:table-frame: ends\n:table-grid: cols\n:table-stripes: even\n\n|===\n|Two\n|===\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let tables = parsed
        .document_storage()
        .blocks
        .iter()
        .filter_map(|block| {
            if let crate::Block::DelimitedBlock(block) = block {
                Some(block)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(tables.len(), 2);
    for (block, expected) in tables.into_iter().zip([
        (TableFrame::Sides, TableGrid::Rows, TableStripes::Odd),
        (TableFrame::Ends, TableGrid::Columns, TableStripes::Even),
    ]) {
        let BlockKind::Table(table) = BlockRef::from(block).kind() else {
            return Err("expected table".into());
        };
        let stored = table.presentation().ok_or("missing stored presentation")?;
        assert_eq!(decoration(stored), expected);
        assert_eq!(
            decoration(resolve(block, &parsed.document_storage().attributes)),
            (TableFrame::Sides, TableGrid::Rows, TableStripes::Odd)
        );
        assert_eq!(table.presentation(), Some(stored));
    }
    let constructed = empty_table();
    let BlockKind::Table(table) = BlockRef::from(&constructed).kind() else {
        return Err("expected constructed table".into());
    };
    assert_eq!(table.presentation(), None);
    assert_eq!(
        decoration(resolve(&constructed, &parsed.document_storage().attributes)),
        (TableFrame::Sides, TableGrid::Rows, TableStripes::Odd)
    );
    assert_eq!(table.presentation(), None);
    Ok(())
}
