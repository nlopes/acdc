use acdc_parser::{
    AttributeValue, CompatibilityMode, ContentAttribute, ElementId, Icon, Location, Options,
    SafeMode, Source,
    ast::{AttributeValueRef, BlockKind, ContentRef, IconRef, InlineKind},
    parse_file,
};

type Error = Box<dyn std::error::Error>;

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
    let mut blocks = parsed.document().blocks();
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
