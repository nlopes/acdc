use crate::{Block, Listing, Options, Paragraph, ParseResult, parse};
use std::error::Error;
type TestResult = Result<(), Box<dyn Error>>;

fn listing(parsed: &ParseResult) -> Result<&Listing<'_>, Box<dyn Error>> {
    parsed
        .document_storage()
        .blocks
        .iter()
        .find_map(|block| {
            let Block::DelimitedListing(block) = block else {
                return None;
            };
            Some(block)
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
    let mut synthetic = Listing::new(original.0.body.clone(), original.0.location.clone())
        .with_metadata(original.0.metadata.clone())?
        .with_title(original.0.title.clone());
    synthetic.set_delimiter(original.0.delimiter);
    synthetic
        .0
        .open_delimiter_location
        .clone_from(&original.0.open_delimiter_location);
    synthetic
        .0
        .close_delimiter_location
        .clone_from(&original.0.close_delimiter_location);
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
