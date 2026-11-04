use std::borrow::Cow;

use acdc_parser::{
    AttributeValue, CompatibilityMode, Document, ElementAttributes, Options, ParseResult,
    TableFrame, TableGrid,
    ast::{AttributeValueRef, BlockKind, BlockRef, BoldRef, InlineImageRef, InlineKind, InlineRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn bold_text(bold: BoldRef<'_>) -> Result<&str, Error> {
    let [InlineKind::Plain(text)] = &*bold
        .content()
        .inlines()
        .map(InlineRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected one bold text node".into());
    };
    Ok(text.text())
}

fn image_in(block: BlockRef<'_>) -> Result<InlineImageRef<'_>, Error> {
    let BlockKind::Paragraph(paragraph) = block.kind() else {
        return Err("expected paragraph".into());
    };
    let [InlineKind::Image(image)] = &*paragraph
        .content()
        .inlines()
        .map(InlineRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected one inline image".into());
    };
    Ok(*image)
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
    let [BlockKind::Section(section)] = &*parsed
        .document_ref()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected one section".into());
    };
    let [
        BlockKind::Paragraph(_),
        BlockKind::Table(table),
        BlockKind::Paragraph(_),
    ] = &*section.blocks().map(BlockRef::kind).collect::<Vec<_>>()
    else {
        return Err("expected paragraph, table, paragraph in source order".into());
    };
    let row = table.rows().next().ok_or("missing table row")?;
    let cell = row.cells().next().ok_or("missing table cell")?;
    let image = image_in(cell.blocks().next().ok_or("missing cell content")?)?;
    let [InlineKind::Bold(alt)] = &*image
        .alt()
        .ok_or("missing alt")?
        .inlines()
        .map(InlineRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected formatted alt".into());
    };
    let [InlineKind::Bold(tooltip)] = &*image
        .tooltip()
        .ok_or("missing tooltip")?
        .inlines()
        .map(InlineRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected formatted tooltip".into());
    };
    assert_eq!(bold_text(*alt)?, "Bird");
    assert_eq!(bold_text(*tooltip)?, "Hover");
    assert_eq!(
        image.attributes().get("width"),
        Some(AttributeValueRef::Text("120"))
    );
    let native = serde_json::to_value(parsed.document_ref())?;
    let stored = native
        .pointer(
            "/blocks/0/blocks/1/content/rows/0/columns/0/content/0/inlines/0/metadata/attributes",
        )
        .and_then(serde_json::Value::as_object)
        .ok_or("missing native image scalar attributes")?;
    assert!(!stored.contains_key("alt"));
    assert!(!stored.contains_key("title"));
    assert_eq!(
        table.attributes().get("frame"),
        Some(AttributeValueRef::Text("none"))
    );
    assert_eq!(
        table.attributes().get("grid"),
        Some(AttributeValueRef::Text("none"))
    );
    let presentation = table.presentation().ok_or("missing table presentation")?;
    assert_eq!(presentation.frame(), TableFrame::None);
    assert_eq!(presentation.grid(), TableGrid::None);
    assert_eq!(alt.location().start().line(), 10);
    assert_eq!(tooltip.location().start().line(), 10);
    assert!(alt.location().start().column() < tooltip.location().start().column());
    assert!(parsed.resolve_location(alt.location()).file.is_none());
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
    let document = parsed.document_ref();
    std::thread::scope(|scope| {
        scope.spawn(move || assert_eq!(document.blocks().count(), 1));
        assert_eq!(document.blocks().count(), 1);
    });
    Ok(())
}

#[test]
fn content_iteration_keeps_independent_front_and_back_cursors() -> Result<(), Error> {
    let parsed = parse("First *bold* last.", &Options::default())?;
    let paragraph = parsed.document_ref().blocks().next().ok_or("paragraph")?;
    let BlockKind::Paragraph(paragraph) = paragraph.kind() else {
        return Err("expected paragraph".into());
    };
    let mut inlines = paragraph.content().inlines();
    assert_eq!(inlines.len(), 3);
    let mut copy = inlines.clone();
    assert!(
        matches!(inlines.next().map(InlineRef::kind), Some(InlineKind::Plain(text)) if text.text() == "First ")
    );
    assert!(
        matches!(inlines.next_back().map(InlineRef::kind), Some(InlineKind::Plain(text)) if text.text() == " last.")
    );
    assert_eq!(inlines.len(), 1);
    assert!(matches!(
        inlines.next().map(InlineRef::kind),
        Some(InlineKind::Bold(_))
    ));
    assert_eq!(inlines.len(), 0);
    assert!(inlines.next_back().is_none());
    assert_eq!(copy.len(), 3);
    assert!(
        matches!(copy.next_back().map(InlineRef::kind), Some(InlineKind::Plain(text)) if text.text() == " last.")
    );
    Ok(())
}
