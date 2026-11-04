use crate::{Block, DelimitedBlock, DelimitedBlockType, Options, Paragraph, ParseResult, parse};
use std::error::Error;
type TestResult = Result<(), Box<dyn Error>>;

fn listing(parsed: &ParseResult) -> Result<&DelimitedBlock<'_>, Box<dyn Error>> {
    parsed
        .document_storage()
        .blocks
        .iter()
        .find_map(|block| {
            let Block::DelimitedBlock(block) = block else {
                return None;
            };
            matches!(block.inner, DelimitedBlockType::DelimitedListing(_)).then_some(block)
        })
        .ok_or_else(|| "expected listing".into())
}

#[test]
fn retained_text_does_not_change_serialization_or_equality() -> TestResult {
    let parsed = parse(
        "[source,bash]\n----\necho hi <1>\n----\n\n[source,bash]\necho hi",
        &Options::default(),
    )?;
    let original = listing(&parsed)?;
    let mut synthetic = DelimitedBlock::new(
        original.inner.clone(),
        original.delimiter,
        original.location.clone(),
    )
    .with_metadata(original.metadata.clone())?
    .with_title(original.title.clone());
    synthetic
        .open_delimiter_location
        .clone_from(&original.open_delimiter_location);
    synthetic
        .close_delimiter_location
        .clone_from(&original.close_delimiter_location);
    assert!(synthetic.source_text().is_none());
    assert_eq!(original, &synthetic);
    assert_eq!(
        serde_json::to_value(original)?,
        serde_json::to_value(synthetic)?
    );

    let Some(Block::Paragraph(original)) = parsed.document_storage().blocks.last() else {
        return Err("expected final source paragraph".into());
    };
    let synthetic = Paragraph::new(original.content.clone(), original.location.clone())
        .with_metadata(original.metadata.clone())?
        .with_title(original.title.clone());
    assert!(synthetic.source_text().is_none());
    assert_eq!(original, &synthetic);
    assert_eq!(
        serde_json::to_value(original)?,
        serde_json::to_value(synthetic)?
    );
    Ok(())
}
