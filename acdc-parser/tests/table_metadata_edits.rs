//! Supported metadata replacement keeps table decoration coherent.

use acdc_parser::{
    DelimitedBlock,
    ast::{AttributeValueRef, BlockKind, BlockRef, TableRef},
};

type Error = Box<dyn std::error::Error>;

fn table<'doc>(block: &'doc DelimitedBlock<'doc>) -> Result<TableRef<'doc>, Error> {
    let BlockKind::Table(table) = BlockRef::from(block).kind() else {
        return Err("expected table".into());
    };
    Ok(table)
}

#[test]
fn constructed_tables_remain_unresolved_after_metadata_edits() -> Result<(), Error> {
    use acdc_parser::{BlockMetadata, DelimitedBlockType, Location, Table};

    let mut block = DelimitedBlock::new(
        DelimitedBlockType::DelimitedTable(Table::new(Vec::new(), Location::default())),
        "|===",
        Location::default(),
    );
    let mut metadata = BlockMetadata::new();
    metadata.attributes.set("frame".into(), "none".into());
    block.set_metadata(metadata)?;
    assert_eq!(table(&block)?.presentation(), None);
    assert_eq!(
        table(&block)?.declared_frame(),
        Some(AttributeValueRef::Text("none"))
    );
    let mut metadata = block.clone_metadata();
    metadata.attributes.remove("frame");
    let block = block.with_metadata(metadata)?;
    assert_eq!(table(&block)?.presentation(), None);
    assert_eq!(table(&block)?.declared_frame(), None);
    Ok(())
}
