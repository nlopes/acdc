use crate::{
    Block, CompatibilityMode, ContentAttribute, Image, Location, Options, Paragraph, Source, Title,
    ast::{
        AttributeValueRef, AttributesRef, BlockKind, BlockRef, ContentRef, ImageRef, ParagraphRef,
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

#[rstest::rstest]
fn parser_tokens_and_identity_follow_existing_mode_semantics(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[[anchor]]\n[role=first,opts=first]\n[role=second,options=second]\n.Title footnote:[Title note.]\nParagraph.\n\n\
         [#image,role=first,opts=first]\nimage::bird.png[Bird,role=second,opts=second]\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document_storage())?;
    let warnings = format!("{:?}", parsed.warnings());
    let footnotes = parsed.document_storage().footnotes.len();
    let references = parsed.document_storage().references.len();
    assert_eq!(footnotes, 1);
    assert!(references > 0);
    let mut blocks = parsed.document().blocks();
    let Some(BlockKind::Paragraph(paragraph)) = blocks.next().map(BlockRef::kind) else {
        return Err("missing paragraph".into());
    };
    let Some(Block::Paragraph(owner)) = parsed.document_storage().blocks.first() else {
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
    let Some(Block::Image(owner)) = parsed.document_storage().blocks.get(1) else {
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
    assert_eq!(serde_json::to_value(parsed.document_storage())?, before);
    assert_eq!(format!("{:?}", parsed.warnings()), warnings);
    assert_eq!(parsed.document_storage().footnotes.len(), footnotes);
    assert_eq!(parsed.document_storage().references.len(), references);
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

fn assert_content(attributes: AttributesRef<'_>, name: &str, content: Option<ContentRef<'_>>) {
    assert_eq!(
        attributes.get(name),
        content.map(AttributeValueRef::Content)
    );
    assert_eq!(attributes.contains(name), content.is_some());
}

#[test]
fn block_image_fields_keep_absent_empty_and_contextual_names_distinct() {
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
}

#[test]
fn block_audio_title_content_suppresses_stale_scalar_metadata() -> Result<(), Error> {
    let mut audio = crate::Audio::new(
        Source::Url(crate::SourceUrl::new("https://EXAMPLE.org:443/music")?),
        Location::default(),
    );
    audio.title = Title::new(Vec::new());
    audio
        .metadata
        .attributes
        .set("title".into(), "stale title".into());
    let read = crate::ast::AudioRef::from(&audio);
    assert!(read.title().is_some_and(ContentRef::is_empty));
    assert_content(read.attributes(), "title", read.title());
    Ok(())
}
