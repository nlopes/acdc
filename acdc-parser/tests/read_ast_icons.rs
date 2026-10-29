use acdc_parser::{
    AttributeValue, Block, CompatibilityMode, ContentAttribute, ElementId, Icon, InlineMacro,
    InlineNode, Location, Options, SafeMode, Source,
    ast::{AttributeValueRef, BlockKind, ContentRef, IconRef, InlineKind, SourceKind},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

fn shared<T: Copy + Send + Sync>(_: T) {}

fn first_icon(content: ContentRef<'_>) -> Result<IconRef<'_>, Error> {
    content
        .inlines()
        .find_map(|inline| {
            if let InlineKind::Icon(icon) = inline.kind() {
                Some(icon)
            } else {
                None
            }
        })
        .ok_or_else(|| "icon".into())
}

fn first_owner<'a, 'b>(document: &'b acdc_parser::Document<'a>) -> Result<&'b Icon<'a>, Error> {
    let Some(Block::Paragraph(paragraph)) = document.blocks.first() else {
        return Err("paragraph".into());
    };
    paragraph
        .content
        .iter()
        .find_map(|inline| {
            if let InlineNode::Macro(InlineMacro::Icon(icon)) = inline {
                Some(icon.as_ref())
            } else {
                None
            }
        })
        .ok_or_else(|| "icon".into())
}

#[rstest::rstest]
fn icon_reads_preserve_identity_content_and_controls(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "é icon:star[2x,id=étoile中,role=\"one two\",opts=\"custom,nofollow\",alt='*Star*',title='_Tip_',custom=kept]";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.warnings(), []);
    let BlockKind::Paragraph(paragraph) = parsed
        .document_ref()
        .blocks()
        .next()
        .ok_or("paragraph")?
        .kind()
    else {
        return Err("paragraph".into());
    };
    let icon = first_icon(paragraph.content())?;
    assert_eq!(icon.target().kind(), SourceKind::Name("star"));
    assert_eq!(icon.size(), Some("2x"));
    let id = icon.id().ok_or("id")?;
    assert_eq!(id.as_str(), "étoile中");
    let start = source.find("étoile中").ok_or("source id")?;
    assert_eq!(id.location().absolute_start(), start);
    assert_eq!(
        id.location().absolute_end(),
        start
            + "étoile中"
                .char_indices()
                .last()
                .ok_or("last ID character")?
                .0
    );
    assert_eq!(
        id.location().start().column(),
        u32::try_from(source[..start].chars().count())? + 1
    );
    assert_eq!(icon.roles().iter().collect::<Vec<_>>(), ["one", "two"]);
    assert_eq!(
        icon.options().iter().collect::<Vec<_>>(),
        ["custom", "nofollow"]
    );
    assert!(matches!(
        icon.alt()
            .ok_or("alt")?
            .inlines()
            .next()
            .ok_or("alt node")?
            .kind(),
        InlineKind::Bold(_)
    ));
    assert!(matches!(
        icon.tooltip()
            .ok_or("tooltip")?
            .inlines()
            .next()
            .ok_or("tooltip node")?
            .kind(),
        InlineKind::Italic(_)
    ));
    let attributes = icon.attributes();
    assert_eq!(
        attributes.get("id"),
        Some(AttributeValueRef::Text("étoile中"))
    );
    assert_eq!(
        attributes.get("roles"),
        Some(AttributeValueRef::Tokens(icon.roles()))
    );
    assert_eq!(
        attributes.get("options"),
        Some(AttributeValueRef::Tokens(icon.options()))
    );
    assert_eq!(
        attributes.get("alt"),
        icon.alt().map(AttributeValueRef::Content)
    );
    assert_eq!(
        attributes.get("title"),
        icon.tooltip().map(AttributeValueRef::Content)
    );
    assert_eq!(
        attributes.get("custom"),
        Some(AttributeValueRef::Text("kept"))
    );
    assert_eq!(attributes.iter().count(), 7);
    for (name, value) in attributes.iter() {
        assert_eq!(attributes.get(name), Some(value));
    }
    let owner = first_owner(parsed.document())?;
    let json = serde_json::to_value(owner)?;
    assert_eq!(
        json,
        serde_json::to_value(InlineNode::Macro(InlineMacro::Icon(Box::new(
            owner.clone()
        ))))?
    );
    assert!(json.get("attributes").is_none());
    assert_eq!(
        json.pointer("/metadata/id/id"),
        Some(&serde_json::json!("étoile中"))
    );
    assert_eq!(
        json.pointer("/metadata/attributes/size"),
        Some(&serde_json::json!("2x"))
    );
    for name in ["alt", "title", "id", "role", "roles", "opts", "options"] {
        assert!(!owner.attributes().contains_key(name));
    }
    shared(icon);
    std::thread::scope(|scope| {
        let options = icon.options().iter();
        scope.spawn(move || assert_eq!(options.count(), 2));
    });
    Ok(())
}

#[rstest::rstest]
fn reserved_icon_attributes_cannot_shadow_dedicated_values(
    #[values("alt", "title", "id", "role", "roles", "opts", "options")] name: &str,
) -> Result<(), Error> {
    let mut icon = Icon::new(Source::Name("star"), Location::default());
    icon.alt = Some(ContentAttribute::default());
    icon.metadata.id = Some(ElementId::new("id", Location::default()));
    icon.metadata.roles.push("role");
    icon.options_mut().push("custom");
    let before = serde_json::to_value(&icon)?;
    assert_eq!(
        icon.set_attribute(name.to_owned().into(), "shadow".into())
            .err()
            .ok_or("reserved write")?
            .name,
        name
    );
    assert_eq!(
        icon.remove_attribute(name)
            .err()
            .ok_or("reserved removal")?
            .name,
        name
    );
    assert_eq!(serde_json::to_value(&icon)?, before);
    Ok(())
}

#[test]
fn constructed_icons_keep_independent_content_and_controls() -> Result<(), Error> {
    let mut icon = Icon::new(Source::Name(""), Location::default());
    assert!(IconRef::from(&icon).attributes().is_empty());
    icon.alt = Some(ContentAttribute::default());
    icon.tooltip = Some(ContentAttribute::default());
    icon.metadata.id = Some(ElementId::new("", Location::default()));
    icon.set_attribute("size".into(), "".into())?;
    icon.set_attribute("custom".into(), AttributeValue::Bool(false))?;
    icon.set_attribute("valueless".into(), AttributeValue::None)?;
    let read = IconRef::from(&icon);
    assert_eq!(read.size(), Some(""));
    assert!(read.alt().ok_or("empty alt")?.is_empty());
    assert!(read.tooltip().ok_or("empty tooltip")?.is_empty());
    assert_eq!(read.id().ok_or("empty id")?.as_str(), "");
    assert_eq!(
        read.attributes().get("custom"),
        Some(AttributeValueRef::Bool(false))
    );
    assert_eq!(
        read.attributes().get("valueless"),
        Some(AttributeValueRef::None)
    );
    icon.tooltip = None;
    icon.metadata = acdc_parser::InlineMetadata::default();
    icon.options_mut().push("extension");
    assert_eq!(
        icon.remove_attribute("custom")?,
        Some(AttributeValue::Bool(false))
    );
    let json = serde_json::to_value(&icon)?;
    assert_eq!(json.get("alt"), Some(&serde_json::json!([])));
    assert!(json.get("tooltip").is_none());
    assert_eq!(
        json.pointer("/metadata/options"),
        Some(&serde_json::json!(["extension"]))
    );
    assert!(json.pointer("/metadata/id").is_none());
    Ok(())
}

#[rstest::rstest]
fn icon_ids_follow_expansion_includes_and_table_cells(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let main = directory.path().join("main.adoc");
    let include = directory.path().join("icons.adoc");
    std::fs::write(
        &main,
        ":icon-id: expanded\n\ninclude::icons.adoc[]\n\n[cols=a]\n|===\n|icon:star[id=cell]\n|===\n",
    )?;
    let source = "A paragraph.\n\né icon:star[id={icon-id},role=++one two++,opts=++first,second++,custom=++a,b++]\n";
    std::fs::write(&include, source)?;
    let parsed = parse_file(
        &main,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let mut blocks = parsed.document_ref().blocks();
    blocks.next().ok_or("first paragraph")?;
    let BlockKind::Paragraph(paragraph) = blocks.next().ok_or("paragraph")?.kind() else {
        return Err("paragraph".into());
    };
    let icon = first_icon(paragraph.content())?;
    let id = icon.id().ok_or("included id")?;
    assert_eq!(id.as_str(), "expanded");
    let resolved = parsed.resolve_location(id.location());
    assert_eq!(resolved.file, Some(include));
    assert_eq!(resolved.location.start.line, 3);
    assert_eq!(
        resolved.location.absolute_start,
        source.find("{icon-id}").ok_or("id reference")?
    );
    assert_eq!(icon.roles().iter().collect::<Vec<_>>(), ["one", "two"]);
    assert_eq!(
        icon.options().iter().collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert_eq!(
        icon.attributes().get("custom"),
        Some(AttributeValueRef::Text("a,b"))
    );
    let BlockKind::Table(table) = blocks.next().ok_or("table")?.kind() else {
        return Err("table".into());
    };
    let cell = table
        .rows()
        .next()
        .ok_or("row")?
        .cells()
        .next()
        .ok_or("cell")?;
    let BlockKind::Paragraph(paragraph) = cell.blocks().next().ok_or("cell paragraph")?.kind()
    else {
        return Err("cell paragraph".into());
    };
    let id = first_icon(paragraph.content())?.id().ok_or("cell id")?;
    assert_eq!(id.as_str(), "cell");
    assert_eq!(id.location().start().line(), 7);
    Ok(())
}
