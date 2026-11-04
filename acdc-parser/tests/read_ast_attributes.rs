use std::borrow::Cow;

use acdc_parser::{
    AttributeValue, CompatibilityMode, ContentAttribute, ElementId, InlineImage, Location, Options,
    Paragraph, Source,
    ast::{
        AttributeValueRef, AttributesRef, BlockKind, BlockRef, ContentRef, InlineImageRef,
        InlineKind, InlineRef, ParagraphRef, TokensRef,
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
    let Some(BlockKind::Section(section)) = parsed.document().blocks().next().map(BlockRef::kind)
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
    let mut metadata = paragraph.take_metadata();
    let name = String::from("custom");
    let name_pointer = name.as_ptr();
    let text = String::from("\"literal quotes\"");
    let text_pointer = text.as_ptr();
    metadata
        .attributes
        .set(Cow::Owned(name), AttributeValue::String(Cow::Owned(text)));
    metadata.attributes.set("empty".into(), "".into());
    metadata.attributes.set("false".into(), false.into());
    metadata.attributes.set("none".into(), AttributeValue::None);
    paragraph.set_metadata(metadata)?;
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
fn image_fields_keep_absent_empty_and_contextual_names_distinct() -> Result<(), Error> {
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

#[rstest::rstest]
fn attribute_locations_keep_recorded_spans_and_absence(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source =
        "Before.\n\n[NOTE,icon=missing.svg]\n====\nBody.\n====\n\nimage:bird.png[id=bird]\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut blocks = parsed.document().blocks();
    assert!(
        blocks
            .next()
            .ok_or("paragraph")?
            .attributes()
            .ok_or("attributes")?
            .location()
            .is_none()
    );
    let BlockKind::Admonition(note) = blocks.next().ok_or("admonition")?.kind() else {
        return Err("expected admonition".into());
    };
    let span = note.attributes().location().ok_or("metadata span")?;
    let start = source.find("[NOTE").ok_or("metadata start")?;
    assert_eq!(span.absolute_start(), start);
    assert_eq!(span.start().line(), 3);
    assert_eq!(span.start().column(), 1);
    assert!(span.absolute_end() < source.find("====").ok_or("delimiter")?);
    assert!(source[start..=span.absolute_end()].starts_with("[NOTE,icon=missing.svg]"));
    assert_ne!(span, note.location());
    let BlockKind::Paragraph(paragraph) = blocks.next().ok_or("inline paragraph")?.kind() else {
        return Err("expected inline paragraph".into());
    };
    let InlineKind::Image(image) = paragraph
        .content()
        .inlines()
        .next()
        .ok_or("inline image")?
        .kind()
    else {
        return Err("expected inline image".into());
    };
    assert!(image.attributes().location().is_none());
    let constructed = Paragraph::new(Vec::new(), Location::default());
    assert!(
        ParagraphRef::from(&constructed)
            .attributes()
            .location()
            .is_none()
    );
    Ok(())
}

#[rstest::rstest]
fn attribute_locations_keep_include_provenance(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let child = directory.path().join("note.adoc");
    std::fs::write(
        &child,
        "éarlier paragraph.\n\n[NOTE,icon=missing.svg]\n====\nBody.\n====\n",
    )?;
    let source = format!("include::{}[]\n", child.display());
    let parsed = parse(
        &source,
        &Options::builder()
            .with_safe_mode(acdc_parser::SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let note = parsed
        .document()
        .blocks()
        .find_map(|block| {
            if let BlockKind::Admonition(note) = block.kind() {
                Some(note)
            } else {
                None
            }
        })
        .ok_or("included note")?;
    let span = note.attributes().location().ok_or("included metadata")?;
    assert_eq!(span.start().line(), 3);
    assert_eq!(span.start().column(), 1);
    assert_eq!(
        span.start()
            .include_chain()
            .ok_or("start chain")?
            .collect::<Vec<_>>(),
        [child.to_str().ok_or("child path")?]
    );
    assert_eq!(
        span.end()
            .include_chain()
            .ok_or("end chain")?
            .collect::<Vec<_>>(),
        [child.to_str().ok_or("child path")?]
    );
    let copied = Location::from(span);
    assert_eq!(span, (&copied).into());
    Ok(())
}
