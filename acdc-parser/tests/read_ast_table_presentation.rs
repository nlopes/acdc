//! Table decoration preserves local declarations and document snapshots.

use acdc_parser::{
    AttributeValue, CompatibilityMode, DelimitedBlock, DelimitedBlockType, DocumentAttributes,
    Location, Options, Table, TableFrame, TableGrid, TablePresentation, TableStripes,
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
fn local_values_mask_defaults_without_changing_their_meaning(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Tables\n:table-frame: sides\n:table-grid: cols\n:table-stripes: even\n\nText.",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    for (local, expected) in [
        (
            None,
            (TableFrame::Sides, TableGrid::Columns, TableStripes::Even),
        ),
        (
            Some(AttributeValue::None),
            (TableFrame::All, TableGrid::All, TableStripes::None),
        ),
        (
            Some(AttributeValue::Bool(false)),
            (TableFrame::All, TableGrid::All, TableStripes::None),
        ),
        (
            Some(AttributeValue::Bool(true)),
            (TableFrame::None, TableGrid::None, TableStripes::None),
        ),
        (
            Some(AttributeValue::from("")),
            (TableFrame::None, TableGrid::None, TableStripes::None),
        ),
        (
            Some(AttributeValue::from("all")),
            (TableFrame::All, TableGrid::All, TableStripes::All),
        ),
        (
            Some(AttributeValue::from("none")),
            (TableFrame::None, TableGrid::None, TableStripes::None),
        ),
        (
            Some(AttributeValue::from("ends")),
            (TableFrame::Ends, TableGrid::None, TableStripes::None),
        ),
        (
            Some(AttributeValue::from("topbot")),
            (TableFrame::Ends, TableGrid::None, TableStripes::None),
        ),
        (
            Some(AttributeValue::from("sides")),
            (TableFrame::Sides, TableGrid::None, TableStripes::None),
        ),
        (
            Some(AttributeValue::from("rows")),
            (TableFrame::None, TableGrid::Rows, TableStripes::None),
        ),
        (
            Some(AttributeValue::from("cols")),
            (TableFrame::None, TableGrid::Columns, TableStripes::None),
        ),
        (
            Some(AttributeValue::from("odd")),
            (TableFrame::None, TableGrid::None, TableStripes::Odd),
        ),
        (
            Some(AttributeValue::from("even")),
            (TableFrame::None, TableGrid::None, TableStripes::Even),
        ),
        (
            Some(AttributeValue::from("hover")),
            (TableFrame::None, TableGrid::None, TableStripes::Hover),
        ),
        (
            Some(AttributeValue::from("INVALID")),
            (TableFrame::None, TableGrid::None, TableStripes::None),
        ),
        (
            Some(AttributeValue::from("'all'")),
            (TableFrame::None, TableGrid::None, TableStripes::None),
        ),
    ] {
        let mut block = empty_table();
        if let Some(local) = local {
            for name in ["frame", "grid", "stripes"] {
                block.metadata.attributes.set(name.into(), local.clone());
            }
        }
        assert_eq!(
            decoration(resolve(&block, &parsed.document().attributes)),
            expected
        );
    }
    Ok(())
}

#[rstest::rstest]
fn document_values_distinguish_absence_presence_unsets_and_exact_text(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    for (entries, expected) in [
        ("", (TableFrame::All, TableGrid::All, TableStripes::None)),
        (
            ":table-frame:\n:table-grid:\n:table-stripes:\n",
            (TableFrame::None, TableGrid::None, TableStripes::None),
        ),
        (
            ":table-frame!:\n:table-grid!:\n:table-stripes!:\n",
            (TableFrame::All, TableGrid::All, TableStripes::None),
        ),
        (
            ":table-frame: sides\n:table-grid: rows\n:table-stripes: odd\n",
            (TableFrame::Sides, TableGrid::Rows, TableStripes::Odd),
        ),
        (
            ":table-frame: INVALID\n:table-grid: INVALID\n:table-stripes: INVALID\n",
            (TableFrame::None, TableGrid::None, TableStripes::None),
        ),
        (
            ":table-frame: 'sides'\n:table-grid: 'rows'\n:table-stripes: 'odd'\n",
            (TableFrame::None, TableGrid::None, TableStripes::None),
        ),
    ] {
        let value = {
            let source = format!("= Tables\n{entries}\nText.");
            let parsed = parse(
                &source,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            let value = resolve(&empty_table(), &parsed.document().attributes);
            assert_eq!(
                TablePresentation::resolve(None, parsed.document_ref().attributes()),
                value
            );
            value
        };
        assert_eq!(decoration(value), expected, "{entries:?}");
    }
    Ok(())
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
        .document()
        .blocks
        .iter()
        .filter_map(|block| {
            if let acdc_parser::Block::DelimitedBlock(block) = block {
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
            decoration(resolve(block, &parsed.document().attributes)),
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
        decoration(resolve(&constructed, &parsed.document().attributes)),
        (TableFrame::Sides, TableGrid::Rows, TableStripes::Odd)
    );
    assert_eq!(table.presentation(), None);
    Ok(())
}
