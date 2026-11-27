//! Supported metadata replacement keeps table decoration coherent.

use acdc_parser::{
    Location,
    ast::{AttributeValueRef, TableRef},
    edit::{BlockMetadata, Table},
};

type Error = Box<dyn std::error::Error>;

#[test]
fn constructed_tables_remain_unresolved_after_metadata_edits() -> Result<(), Error> {
    let mut block = Table::new(Vec::new(), Location::default());
    let mut metadata = BlockMetadata::new();
    metadata.attributes.set("frame".into(), "none".into());
    block.set_metadata(metadata)?;
    assert_eq!(TableRef::from(&block).presentation(), None);
    assert_eq!(
        TableRef::from(&block).declared_frame(),
        Some(AttributeValueRef::Text("none"))
    );
    let mut metadata = block.clone_metadata();
    metadata.attributes.remove("frame");
    let block = block.with_metadata(metadata)?;
    assert_eq!(TableRef::from(&block).presentation(), None);
    assert_eq!(TableRef::from(&block).declared_frame(), None);
    Ok(())
}
