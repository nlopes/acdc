use std::borrow::Cow;

use acdc_parser::{
    AttributeValue, Block, Bold, CompatibilityMode, DelimitedBlockType, Document,
    ElementAttributes, InlineImage, InlineMacro, InlineNode, Options, ParseResult, TableFrame,
    TableGrid, parse,
};

type Error = Box<dyn std::error::Error>;

fn bold_text<'a>(bold: &'a Bold<'_>) -> Result<&'a str, Error> {
    let [InlineNode::PlainText(text)] = bold.content.as_slice() else {
        return Err("expected one bold text node".into());
    };
    Ok(text.content)
}

fn image_in<'a>(block: &'a Block<'_>) -> Result<&'a InlineImage<'a>, Error> {
    let Block::Paragraph(paragraph) = block else {
        return Err("expected paragraph".into());
    };
    let [InlineNode::Macro(InlineMacro::Image(image))] = paragraph.content.as_slice() else {
        return Err("expected one inline image".into());
    };
    Ok(image)
}

#[rstest::rstest]
fn nested_reads_preserve_structure_content_and_declarations(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = {
        let input = String::from(
            "= Read contract\n\n== Section\n\nBefore.\n\n\
             [cols=a,frame=none,grid=none]\n|===\n|\n\
             image:photo.png[alt='*Bird*',title='*Hover*',width=120]\n|===\n\nAfter.\n",
        );
        parse(
            &input,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?
    };
    let [Block::Section(section)] = parsed.document().blocks.as_slice() else {
        return Err("expected one section".into());
    };
    let [
        Block::Paragraph(_),
        Block::DelimitedBlock(block),
        Block::Paragraph(_),
    ] = section.content.as_slice()
    else {
        return Err("expected paragraph, table, paragraph in source order".into());
    };
    let DelimitedBlockType::DelimitedTable(table) = &block.inner else {
        return Err("expected table".into());
    };
    let row = table.rows.first().ok_or("missing table row")?;
    let cell = row.columns.first().ok_or("missing table cell")?;
    let image = image_in(cell.content.first().ok_or("missing cell content")?)?;
    let [InlineNode::BoldText(alt)] = image.alt.as_deref().ok_or("missing alt")? else {
        return Err("expected formatted alt".into());
    };
    let [InlineNode::BoldText(tooltip)] = image.tooltip.as_deref().ok_or("missing tooltip")? else {
        return Err("expected formatted tooltip".into());
    };
    assert_eq!(bold_text(alt)?, "Bird");
    assert_eq!(bold_text(tooltip)?, "Hover");
    assert_eq!(image.attributes().get("width"), Some(&"120".into()));
    assert!(!image.attributes().contains_key("alt"));
    assert!(!image.attributes().contains_key("title"));
    assert_eq!(block.metadata.attributes.get("frame"), Some(&"none".into()));
    assert_eq!(block.metadata.attributes.get("grid"), Some(&"none".into()));
    let presentation = table.presentation().ok_or("missing table presentation")?;
    assert_eq!(presentation.frame(), TableFrame::None);
    assert_eq!(presentation.grid(), TableGrid::None);
    assert_eq!(alt.location.start.line, 10);
    assert_eq!(tooltip.location.start.line, 10);
    assert!(alt.location.start.column < tooltip.location.start.column);
    assert!(parsed.source_location(&alt.location).file.is_none());
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[test]
fn scalar_reads_borrow_exact_owned_text() -> Result<(), Error> {
    let mut attributes: ElementAttributes<'static> = ElementAttributes::default();
    let value = String::from("\"literal quotes\"");
    let pointer = value.as_ptr();
    attributes.set("custom".into(), AttributeValue::String(Cow::Owned(value)));
    attributes.set("empty".into(), "".into());
    attributes.set("disabled".into(), false.into());
    attributes.set("none".into(), AttributeValue::None);

    let Some(AttributeValue::String(value)) = attributes.get("custom") else {
        return Err("expected scalar text".into());
    };
    let borrowed: &str = value.as_ref();
    assert_eq!(borrowed, "\"literal quotes\"");
    assert_eq!(borrowed.as_ptr(), pointer);
    assert_eq!(attributes.get("empty"), Some(&"".into()));
    assert_eq!(attributes.get("disabled"), Some(&false.into()));
    assert_eq!(attributes.get("none"), Some(&AttributeValue::None));
    assert_eq!(attributes.get("missing"), None);
    for (name, value) in attributes.iter() {
        assert_eq!(attributes.get(name), Some(value));
    }
    Ok(())
}

#[test]
fn callers_can_share_immutable_document_reads() -> Result<(), Error> {
    fn assert_send<T: Send>() {}
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send::<ParseResult>();
    assert_send_sync::<Document<'_>>();

    let parsed = parse("A paragraph.", &Options::default())?;
    let document = parsed.document();
    std::thread::scope(|scope| {
        scope.spawn(move || assert_eq!(document.blocks.len(), 1));
        assert_eq!(document.blocks.len(), 1);
    });
    Ok(())
}
