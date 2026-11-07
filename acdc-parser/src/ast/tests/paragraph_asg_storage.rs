use crate::{AttributeValue, Block, Document, Location, Paragraph, asg};

#[test]
fn dedicated_metadata_cannot_be_shadowed_by_scalar_attributes() {
    let mut paragraph = Paragraph::new(Vec::new(), Location::default());
    paragraph.metadata.style = Some("example");
    paragraph
        .metadata
        .attributes
        .set("style".into(), AttributeValue::String("source".into()));
    let mut document = Document::default();
    document.push_block(Block::Paragraph(paragraph));
    assert!(
        matches!(asg::document((&document).into()), Err(asg::Error::Unsupported {path, ..}) if path == "/blocks/0/metadata/attributes/style")
    );
}
