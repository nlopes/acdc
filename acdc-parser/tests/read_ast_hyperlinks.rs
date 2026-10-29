use acdc_parser::{
    CompatibilityMode, ContentAttribute, ElementAttributes, InlineMacro, InlineNode, Link,
    Location, Mailto, Options, Source, Url,
    ast::{AttributeValueRef, InlineKind, InlineRef, LinkRef, MailtoRef, UrlRef},
    parse_inline,
};

type Error = Box<dyn std::error::Error>;

fn shared<T: Copy + Send + Sync>(_: T) {}

#[rstest::rstest]
fn hyperlink_reads_preserve_metadata_and_content(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("link:guide.adoc", "https://example.org", "mailto:team@example.org")] target: &str,
) -> Result<(), Error> {
    let source = format!(
        "{target}[*Label*,id=étoile中,role=\"one two\",opts=\"first,second\",title='_Tip_',window=reading,custom=kept]"
    );
    let parsed = parse_inline(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [inline] = parsed.inlines() else {
        return Err("one inline".into());
    };
    macro_rules! check {
        ($node:expr) => {{
            let node = $node;
            shared(node);
            assert!(matches!(
                node.text().inlines().next().ok_or("label")?.kind(),
                InlineKind::Bold(_)
            ));
            assert!(matches!(
                node.tooltip()
                    .ok_or("tooltip")?
                    .inlines()
                    .next()
                    .ok_or("tip")?
                    .kind(),
                InlineKind::Italic(_)
            ));
            assert_eq!(node.window(), Some("reading"));
            assert_eq!(node.roles().iter().collect::<Vec<_>>(), ["one", "two"]);
            assert_eq!(
                node.options().iter().collect::<Vec<_>>(),
                ["first", "second"]
            );
            let id = node.id().ok_or("ID")?;
            assert_eq!(id.as_str(), "étoile中");
            let start = source.find("étoile中").ok_or("ID source")?;
            assert_eq!(id.location().absolute_start(), start);
            assert_eq!(id.location().absolute_end(), start + "étoile".len());
            let attributes = node.attributes();
            assert_eq!(
                attributes.get("title"),
                node.tooltip().map(AttributeValueRef::Content)
            );
            assert_eq!(attributes.get("roles"), attributes.get("role"));
            assert_eq!(attributes.get("options"), attributes.get("opts"));
            assert_eq!(
                attributes.get("custom"),
                Some(AttributeValueRef::Text("kept"))
            );
            for (name, value) in attributes.iter() {
                assert_eq!(attributes.get(name), Some(value));
                assert_eq!(attributes.iter().filter(|(key, _)| *key == name).count(), 1);
            }
            shared(node.target());
        }};
    }
    let kind = InlineRef::from(inline).kind();
    if let InlineKind::Link(node) = kind {
        check!(node);
    } else if let InlineKind::Url(node) = kind {
        check!(node);
    } else if let InlineKind::Mailto(node) = kind {
        check!(node);
    } else {
        return Err("hyperlink".into());
    }
    let tree = serde_json::to_value(inline)?;
    let direct = if let InlineNode::Macro(InlineMacro::Link(node)) = inline {
        serde_json::to_value(node)?
    } else if let InlineNode::Macro(InlineMacro::Url(node)) = inline {
        serde_json::to_value(node)?
    } else if let InlineNode::Macro(InlineMacro::Mailto(node)) = inline {
        serde_json::to_value(node)?
    } else {
        return Err("hyperlink owner".into());
    };
    assert_eq!(direct, tree);
    assert_eq!(
        tree.pointer("/metadata/attributes/custom"),
        Some(&serde_json::json!("kept"))
    );
    assert!(tree.get("attributes").is_none());
    Ok(())
}

#[test]
fn mail_payload_remains_unencoded_and_separate_from_named_extensions() -> Result<(), Error> {
    for (source, subject, body) in [
        ("mailto:team@example.org[]", None, None),
        ("mailto:team@example.org[,\"\",\"\"]", Some(""), Some("")),
        (
            "mailto:team@example.org[Team,Hello & café,Body + text]",
            Some("Hello & café"),
            Some("Body + text"),
        ),
        (
            "mailto:team@example.org[Team,subject=extension,body=extension]",
            None,
            None,
        ),
    ] {
        let parsed = parse_inline(source, &Options::default())?;
        let [InlineNode::Macro(InlineMacro::Mailto(owner))] = parsed.inlines() else {
            return Err("mailto".into());
        };
        let mail = MailtoRef::from(owner.as_ref());
        assert_eq!(mail.subject(), subject);
        assert_eq!(mail.body(), body);
        let json = serde_json::to_value(parsed.inlines().first().ok_or("mailto inline")?)?;
        assert_eq!(
            json.get("subject"),
            subject.map(serde_json::Value::from).as_ref()
        );
        assert_eq!(json.get("body"), body.map(serde_json::Value::from).as_ref());
        if source.contains("subject=extension") {
            assert_eq!(
                mail.attributes().get("subject"),
                Some(AttributeValueRef::Text("extension"))
            );
        }
    }
    Ok(())
}

#[rstest::rstest]
fn dedicated_hyperlink_attributes_cannot_be_shadowed(
    #[values("title", "id", "role", "roles", "opts", "options")] name: &str,
) -> Result<(), Error> {
    macro_rules! check {
        ($ty:ident, $read:ident) => {{
            let mut owner = $ty::new(Source::Name("target"), Location::default());
            assert_eq!(
                owner
                    .set_attribute(name.into(), "shadow".into())
                    .err()
                    .ok_or("rejected set")?
                    .name,
                name
            );
            assert_eq!(
                owner
                    .remove_attribute(name)
                    .err()
                    .ok_or("rejected remove")?
                    .name,
                name
            );
            assert!($read::from(&owner).attributes().is_empty());
            owner.set_attribute("custom".into(), "".into())?;
            owner.tooltip = Some(ContentAttribute::default());
            owner.metadata.roles.push("one");
            owner.options_mut().push("first");
            let read = $read::from(&owner);
            assert!(read.tooltip().ok_or("empty tooltip")?.is_empty());
            assert_eq!(
                read.attributes().get("custom"),
                Some(AttributeValueRef::Text(""))
            );
            assert_eq!(read.roles().iter().collect::<Vec<_>>(), ["one"]);
            assert_eq!(read.options().iter().collect::<Vec<_>>(), ["first"]);
        }};
    }
    check!(Link, LinkRef);
    check!(Url, UrlRef);
    check!(Mailto, MailtoRef);
    let mut map = ElementAttributes::default();
    map.set(name.into(), "shadow".into());
    assert!(
        Link::new(Source::Name("target"), Location::default())
            .with_attributes(map)
            .is_err()
    );
    Ok(())
}

#[test]
fn empty_link_text_keeps_automatic_display_policy() -> Result<(), Error> {
    let parsed = acdc_parser::parse(
        ":hide-uri-scheme:\n\nlink:https://example.org[title=\"\"]",
        &Options::default(),
    )?;
    let Some(acdc_parser::Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
        return Err("paragraph".into());
    };
    let InlineKind::Link(link) =
        InlineRef::from(paragraph.content.first().ok_or("link inline")?).kind()
    else {
        return Err("link".into());
    };
    assert!(link.text().is_empty());
    assert!(link.hides_uri_scheme());
    assert!(link.tooltip().ok_or("empty tooltip")?.is_empty());
    Ok(())
}

#[test]
fn transferring_scalar_controls_preserves_dedicated_metadata() -> Result<(), Error> {
    let mut link = Link::new(Source::Name("guide.adoc"), Location::default());
    link.metadata.roles.push("resource");
    link.options_mut().push("nofollow");
    link.tooltip = Some(ContentAttribute::default());
    link.set_attribute("old".into(), "discarded".into())?;
    let mut controls = ElementAttributes::default();
    controls.set("window".into(), "reading".into());
    let mut link = link.with_attributes(controls)?;
    assert!(!link.attributes().contains_key("old"));
    assert_eq!(LinkRef::from(&link).window(), Some("reading"));
    assert_eq!(link.metadata.roles, ["resource"]);
    assert_eq!(link.options(), ["nofollow"]);
    assert!(link.tooltip.as_ref().ok_or("tooltip")?.is_empty());
    assert!(link.remove_attribute("window")?.is_some());
    assert!(LinkRef::from(&link).window().is_none());
    assert_eq!(link.metadata.roles, ["resource"]);
    Ok(())
}
