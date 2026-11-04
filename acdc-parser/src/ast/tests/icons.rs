use crate::{
    Block, CompatibilityMode, Icon, InlineMacro, InlineNode, Options,
    ast::{AttributeValueRef, BlockKind, ContentRef, IconRef, InlineKind, SourceKind},
    parse,
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

fn first_owner<'a, 'b>(document: &'b crate::Document<'a>) -> Result<&'b Icon<'a>, Error> {
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
    let BlockKind::Paragraph(paragraph) =
        parsed.document().blocks().next().ok_or("paragraph")?.kind()
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
    let owner = first_owner(parsed.document_storage())?;
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
