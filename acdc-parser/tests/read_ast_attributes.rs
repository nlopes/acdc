use std::borrow::Cow;

use acdc_parser::{
    AttributeValue, Block, CompatibilityMode, ContentAttribute, ElementId, Image, InlineImage,
    Location, Options, Paragraph, Source, Title,
    ast::{
        AttributeValueRef, AttributesRef, BlockKind, BlockRef, ContentRef, ImageRef,
        InlineImageRef, InlineKind, InlineRef, ParagraphRef, TokensRef,
    },
    parse,
};

type Error = Box<dyn std::error::Error>;

fn assert_inventory(attributes: AttributesRef<'_>) {
    let entries = attributes.iter().collect::<Vec<_>>();
    for (index, (name, value)) in entries.iter().enumerate() {
        assert!(attributes.contains(name));
        assert_eq!(attributes.get(name), Some(*value), "{name}");
        assert!(
            !entries
                .iter()
                .take(index)
                .any(|(previous, _)| previous == name)
        );
        assert!(!matches!(*name, "roles" | "options"));
    }
    assert_eq!(attributes.is_empty(), entries.is_empty());
    for (alias, canonical) in [("roles", "role"), ("options", "opts")] {
        assert_eq!(attributes.get(alias), attributes.get(canonical));
        assert_eq!(attributes.contains(alias), attributes.contains(canonical));
    }
    assert_eq!(attributes.get("missing"), None);
    assert!(!attributes.contains("missing"));
}

fn assert_content(attributes: AttributesRef<'_>, name: &str, content: Option<ContentRef<'_>>) {
    assert_eq!(
        attributes.get(name),
        content.map(AttributeValueRef::Content)
    );
    assert_eq!(attributes.contains(name), content.is_some());
}

#[rstest::rstest]
fn nested_callers_read_typed_and_generic_content_consistently(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Attributes\n\n== Section\n\n[cols=a,frame=none,grid=none]\n.Table title\n|===\n|\n\
         .Paragraph title\nimage:bird.png[alt='*Bird*',title='*Hover*',width=120,role='one two',opts='nofollow,noopener',id=bird]\n|===\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(BlockKind::Section(section)) =
        parsed.document_ref().blocks().next().map(BlockRef::kind)
    else {
        return Err("expected section".into());
    };
    assert_inventory(section.attributes());
    assert_content(section.attributes(), "title", section.title());
    let Some(BlockKind::Table(table)) = section.blocks().next().map(BlockRef::kind) else {
        return Err("expected table".into());
    };
    assert_inventory(table.attributes());
    assert_content(table.attributes(), "title", table.title());
    assert_eq!(table.attributes().get("frame"), table.declared_frame());
    assert_eq!(table.attributes().get("grid"), table.declared_grid());
    assert_eq!(table.attributes().get("stripes"), table.declared_stripes());
    let cell = table
        .rows()
        .next()
        .ok_or("missing row")?
        .cells()
        .next()
        .ok_or("missing cell")?;
    let Some(BlockKind::Paragraph(paragraph)) = cell.blocks().next().map(BlockRef::kind) else {
        return Err("expected paragraph".into());
    };
    assert_inventory(paragraph.attributes());
    assert_content(paragraph.attributes(), "title", paragraph.title());
    let Some(InlineKind::Image(image)) = paragraph.content().inlines().next().map(InlineRef::kind)
    else {
        return Err("expected inline image".into());
    };
    assert_inventory(image.attributes());
    assert_content(image.attributes(), "alt", image.alt());
    assert_content(image.attributes(), "title", image.tooltip());
    let Some(AttributeValueRef::Content(alt)) = image.attributes().get("alt") else {
        return Err("expected content attribute".into());
    };
    assert!(matches!(
        alt.inlines().next().map(InlineRef::kind),
        Some(InlineKind::Bold(_))
    ));
    assert_eq!(
        image.attributes().get("width"),
        Some(AttributeValueRef::Text("120"))
    );
    assert_eq!(
        image.attributes().get("id"),
        image.id().map(|id| AttributeValueRef::Text(id.as_str()))
    );
    assert_eq!(image.roles().iter().collect::<Vec<_>>(), ["one", "two"]);
    assert_eq!(
        image.options().iter().collect::<Vec<_>>(),
        ["nofollow", "noopener"]
    );
    assert_eq!(
        image.attributes().get("roles"),
        Some(AttributeValueRef::Tokens(image.roles()))
    );
    assert_eq!(
        image.attributes().get("options"),
        Some(AttributeValueRef::Tokens(image.options()))
    );
    Ok(())
}

#[test]
fn scalar_values_and_owned_names_are_read_without_cloning_or_stripping_quotes() -> Result<(), Error>
{
    let mut paragraph = Paragraph::new(Vec::new(), Location::default());
    let name = String::from("custom");
    let name_pointer = name.as_ptr();
    let text = String::from("\"literal quotes\"");
    let text_pointer = text.as_ptr();
    paragraph
        .metadata
        .attributes
        .set(Cow::Owned(name), AttributeValue::String(Cow::Owned(text)));
    paragraph.metadata.attributes.set("empty".into(), "".into());
    paragraph
        .metadata
        .attributes
        .set("false".into(), false.into());
    paragraph
        .metadata
        .attributes
        .set("none".into(), AttributeValue::None);
    let attributes = ParagraphRef::from(&paragraph).attributes();
    assert_inventory(attributes);
    let Some(AttributeValueRef::Text(text)) = attributes.get(&String::from("custom")) else {
        return Err("expected scalar text".into());
    };
    assert_eq!(text, "\"literal quotes\"");
    assert_eq!(text.as_ptr(), text_pointer);
    let (name, value) = attributes
        .iter()
        .find(|(name, _)| *name == "custom")
        .ok_or("missing custom entry")?;
    assert_eq!(name.as_ptr(), name_pointer);
    assert_eq!(value, AttributeValueRef::Text(text));
    assert_eq!(attributes.get("empty"), Some(AttributeValueRef::Text("")));
    assert_eq!(
        attributes.get("false"),
        Some(AttributeValueRef::Bool(false))
    );
    assert_eq!(attributes.get("none"), Some(AttributeValueRef::None));
    assert_eq!(attributes.iter().count(), 4);
    Ok(())
}

#[test]
fn dedicated_presence_suppresses_stale_map_entries() {
    let mut paragraph = Paragraph::new(Vec::new(), Location::default());
    for name in ["title", "id", "role", "roles", "opts", "options", "style"] {
        paragraph
            .metadata
            .attributes
            .set(name.into(), "stale".into());
    }
    let empty = ParagraphRef::from(&paragraph);
    assert_inventory(empty.attributes());
    assert!(empty.attributes().is_empty());
    assert!(empty.id().is_none());
    assert!(empty.roles().is_empty());
    assert!(empty.options().is_empty());
    assert_eq!(empty.style(), None);

    paragraph.title = Title::new(Vec::new());
    paragraph.metadata.roles = vec!["one", "one", "two"];
    paragraph.metadata.options = vec!["first", "second"];
    paragraph.metadata.style = Some("");
    let node = ParagraphRef::from(&paragraph);
    assert_inventory(node.attributes());
    assert_content(node.attributes(), "title", node.title());
    assert!(node.title().is_some_and(ContentRef::is_empty));
    assert_eq!(
        node.attributes().get("style"),
        Some(AttributeValueRef::Text(""))
    );
    assert_eq!(
        node.roles().iter().collect::<Vec<_>>(),
        ["one", "one", "two"]
    );
    assert!(node.roles().contains("one"));
    assert!(!node.roles().contains("missing"));
    assert_eq!(
        node.attributes().get("role"),
        Some(AttributeValueRef::Tokens(node.roles()))
    );
    assert_eq!(
        node.attributes().get("opts"),
        Some(AttributeValueRef::Tokens(node.options()))
    );
    assert_eq!(node.attributes().iter().count(), 4);
}

#[test]
fn image_fields_keep_absent_empty_and_contextual_names_distinct() -> Result<(), Error> {
    let mut block = Image::new(Source::Name("bird"), Location::default());
    block.metadata.attributes.set("alt".into(), "stale".into());
    assert!(ImageRef::from(&block).attributes().is_empty());
    block.alt = Some(ContentAttribute::default());
    block.title = Title::new(Vec::new());
    let image = ImageRef::from(&block);
    assert_inventory(image.attributes());
    assert_content(image.attributes(), "alt", image.alt());
    assert_content(image.attributes(), "title", image.title());
    assert_eq!(image.attributes().iter().count(), 2);

    let mut inline = InlineImage::new(Source::Name("bird"), Location::default());
    assert!(InlineImageRef::from(&inline).attributes().is_empty());
    inline.tooltip = Some(ContentAttribute::default());
    inline.metadata.id = Some(ElementId::new("", Location::default()));
    inline.set_attribute("tooltip".into(), "extension".into())?;
    inline.set_attribute("style".into(), "".into())?;
    inline.set_attribute("source".into(), "another extension".into())?;
    let image = InlineImageRef::from(&inline);
    assert_inventory(image.attributes());
    assert_content(image.attributes(), "title", image.tooltip());
    assert_content(image.attributes(), "alt", image.alt());
    assert_eq!(
        image.attributes().get("tooltip"),
        Some(AttributeValueRef::Text("extension"))
    );
    assert_eq!(
        image.attributes().get("style"),
        Some(AttributeValueRef::Text(""))
    );
    assert_eq!(
        image.attributes().get("source"),
        Some(AttributeValueRef::Text("another extension"))
    );
    assert_eq!(
        image.attributes().get("id"),
        Some(AttributeValueRef::Text(""))
    );
    for name in ["title", "alt", "id", "roles", "options"] {
        assert!(inline.set_attribute(name.into(), "shadow".into()).is_err());
    }
    inline.tooltip = None;
    assert!(!InlineImageRef::from(&inline).attributes().contains("title"));
    Ok(())
}

#[rstest::rstest]
fn parser_tokens_and_identity_follow_existing_mode_semantics(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[[anchor]]\n[role=first,opts=first]\n[role=second,options=second]\n.Title footnote:[Title note.]\nParagraph.\n\n\
         [#image,role=first,opts=first]\nimage::bird.png[Bird,role=second,opts=second]\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let warnings = format!("{:?}", parsed.warnings());
    let footnotes = parsed.document().footnotes.len();
    let references = parsed.document().references.len();
    assert_eq!(footnotes, 1);
    assert!(references > 0);
    let mut blocks = parsed.document_ref().blocks();
    let Some(BlockKind::Paragraph(paragraph)) = blocks.next().map(BlockRef::kind) else {
        return Err("missing paragraph".into());
    };
    let Some(Block::Paragraph(owner)) = parsed.document().blocks.first() else {
        return Err("missing paragraph owner".into());
    };
    assert_eq!(
        paragraph.roles().iter().collect::<Vec<_>>(),
        owner.metadata.roles
    );
    assert_eq!(
        paragraph.options().iter().collect::<Vec<_>>(),
        owner.metadata.options
    );
    assert_eq!(
        paragraph.attributes().get("id"),
        Some(AttributeValueRef::Text("anchor"))
    );
    assert_eq!(
        paragraph.attributes().get("style"),
        paragraph.style().map(AttributeValueRef::Text)
    );
    assert_inventory(paragraph.attributes());
    let Some(BlockKind::Image(image)) = blocks.next().map(BlockRef::kind) else {
        return Err("missing image".into());
    };
    let Some(Block::Image(owner)) = parsed.document().blocks.get(1) else {
        return Err("missing image owner".into());
    };
    assert_eq!(
        image.roles().iter().collect::<Vec<_>>(),
        owner.metadata.roles
    );
    assert_eq!(
        image.options().iter().collect::<Vec<_>>(),
        owner.metadata.options
    );
    assert_inventory(image.attributes());
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    assert_eq!(format!("{:?}", parsed.warnings()), warnings);
    assert_eq!(parsed.document().footnotes.len(), footnotes);
    assert_eq!(parsed.document().references.len(), references);
    Ok(())
}

#[test]
fn handles_and_iterators_support_shared_reads_after_temporary_handles() {
    fn handle<T: Copy + Send + Sync>() {}
    fn iterator<I: Iterator + Send + Sync>(value: I) -> I {
        value
    }
    handle::<AttributesRef<'_>>();
    handle::<AttributeValueRef<'_>>();
    handle::<TokensRef<'_>>();
    let mut image = InlineImage::new(Source::Name("bird"), Location::default());
    image.metadata.roles = vec!["one", "two"];
    let entries = iterator(InlineImageRef::from(&image).attributes().iter());
    let tokens = iterator(InlineImageRef::from(&image).roles().iter());
    std::thread::scope(|scope| {
        scope.spawn(move || assert_eq!(entries.count(), 1));
        scope.spawn(move || assert_eq!(tokens.count(), 2));
    });
}
