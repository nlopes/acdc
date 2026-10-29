use acdc_parser::{
    CompatibilityMode, CrossReference, InlineMacro, InlineNode, Location, Options,
    XrefCaptionLabel, XrefSignifier, XrefStyle,
    ast::{AttributeValueRef, CrossReferenceRef, InlineKind, InlineRef},
    parse_inline,
};

type Error = Box<dyn std::error::Error>;

fn shared<T: Copy + Send + Sync>(_: T) {}

#[rstest::rstest]
fn cross_reference_reads_keep_labels_and_controls_separate(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "xref:target[*Label*,id=référence中,role=\"one two\",opts=\"a,b\",xrefstyle=short,title=\"Hover *text*\",reftext=Other,custom=kept]";
    let parsed = parse_inline(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [inline @ InlineNode::Macro(InlineMacro::CrossReference(owner))] = parsed.inlines() else {
        return Err("one xref".into());
    };
    let InlineKind::CrossReference(node) = InlineRef::from(inline).kind() else {
        return Err("xref read".into());
    };
    shared(node);
    assert_eq!(node.target(), "target");
    assert!(node.target_is_local());
    assert!(matches!(
        node.text().inlines().next().ok_or("label")?.kind(),
        InlineKind::Bold(_)
    ));
    assert_eq!(node.xrefstyle(), XrefStyle::Short);
    assert_eq!(node.roles().iter().collect::<Vec<_>>(), ["one", "two"]);
    assert_eq!(node.options().iter().collect::<Vec<_>>(), ["a", "b"]);
    let id = node.id().ok_or("ID")?;
    assert_eq!(id.as_str(), "référence中");
    let start = source.find("référence中").ok_or("ID source")?;
    assert_eq!(id.location().absolute_start(), start);
    assert_eq!(id.location().absolute_end(), start + "référence".len());
    let attributes = node.attributes();
    for (key, value) in [
        ("title", "Hover *text*"),
        ("reftext", "Other"),
        ("custom", "kept"),
        ("xrefstyle", "short"),
    ] {
        assert_eq!(attributes.get(key), Some(AttributeValueRef::Text(value)));
    }
    assert_eq!(attributes.get("role"), attributes.get("roles"));
    assert_eq!(attributes.get("opts"), attributes.get("options"));
    for (name, value) in attributes.iter() {
        assert_eq!(attributes.get(name), Some(value));
        assert_eq!(attributes.iter().filter(|(key, _)| *key == name).count(), 1);
    }
    let tree = serde_json::to_value(inline)?;
    assert_eq!(tree, serde_json::to_value(owner)?);
    assert_eq!(
        tree.pointer("/metadata/attributes/title"),
        Some(&serde_json::json!("Hover *text*"))
    );
    assert_eq!(tree.get("xrefstyle"), Some(&serde_json::json!("short")));
    assert_eq!(tree.get("target_is_local"), Some(&serde_json::json!(true)));
    assert!(tree.get("tooltip").is_none());
    Ok(())
}

#[test]
fn xref_attributes_do_not_supply_an_explicit_label() -> Result<(), Error> {
    for (source, local, style) in [
        (
            "xref:target[title=Tip,reftext=Other]",
            true,
            XrefStyle::Default,
        ),
        (
            "xref:other.adoc#section[window=reading,xrefstyle=full]",
            false,
            XrefStyle::Full,
        ),
        ("xref:target[xrefstyle=unknown]", true, XrefStyle::Basic),
    ] {
        let parsed = parse_inline(source, &Options::default())?;
        let [InlineNode::Macro(InlineMacro::CrossReference(owner))] = parsed.inlines() else {
            return Err("xref".into());
        };
        let node = CrossReferenceRef::from(owner.as_ref());
        assert!(node.text().is_empty());
        assert_eq!(node.target_is_local(), local);
        assert_eq!(node.xrefstyle(), style);
    }
    Ok(())
}

#[test]
fn xref_controls_preserve_empty_values_and_restore_protected_text() -> Result<(), Error> {
    let parsed = acdc_parser::parse(
        "xref:target[id=\"\",role=+two roles+,opts=+one,two+,custom=+a,b]+,title=\"\"]",
        &Options::default(),
    )?;
    let [acdc_parser::Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
        return Err("paragraph".into());
    };
    let [InlineNode::Macro(InlineMacro::CrossReference(owner))] = paragraph.content.as_slice()
    else {
        return Err("xref".into());
    };
    let node = CrossReferenceRef::from(owner.as_ref());
    assert_eq!(node.id().ok_or("empty ID")?.as_str(), "");
    assert_eq!(node.roles().iter().collect::<Vec<_>>(), ["two", "roles"]);
    assert_eq!(node.options().iter().collect::<Vec<_>>(), ["one", "two"]);
    assert_eq!(
        node.attributes().get("custom"),
        Some(AttributeValueRef::Text("a,b]"))
    );
    assert_eq!(
        node.attributes().get("title"),
        Some(AttributeValueRef::Text(""))
    );
    Ok(())
}

#[test]
fn xref_updates_have_one_owner_and_keep_reference_policy() -> Result<(), Error> {
    let mut owner = CrossReference::new("target", Location::default());
    assert!(CrossReferenceRef::from(&owner).attributes().is_empty());
    for name in ["id", "role", "roles", "opts", "options", "xrefstyle"] {
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
    }
    owner.metadata.roles.push("resource");
    owner.options_mut().push("nofollow");
    owner.set_attribute("title".into(), "".into())?;
    owner.set_attribute("reftext".into(), "extension".into())?;
    owner.xrefstyle = XrefStyle::Full;
    owner.caption_label = XrefCaptionLabel::NumberOnly;
    owner.set_signifier(XrefSignifier::AtReference("Part"));
    let node = CrossReferenceRef::from(&owner);
    assert_eq!(
        node.attributes().get("xrefstyle"),
        Some(AttributeValueRef::Text("full"))
    );
    assert_eq!(node.caption_label(), XrefCaptionLabel::NumberOnly);
    assert_eq!(node.signifier(), XrefSignifier::AtReference("Part"));
    assert_eq!(node.roles().iter().collect::<Vec<_>>(), ["resource"]);
    assert_eq!(node.options().iter().collect::<Vec<_>>(), ["nofollow"]);
    assert!(node.text().is_empty());
    owner.remove_attribute("title")?;
    assert!(
        !CrossReferenceRef::from(&owner)
            .attributes()
            .contains("title")
    );
    Ok(())
}

#[test]
fn xref_source_aliases_share_token_ownership() -> Result<(), Error> {
    let parsed = parse_inline(
        "xref:target[role=old,roles=\"one two\",id=first,id=last,opts=old,options=\"new,other\"]",
        &Options::default(),
    )?;
    let [InlineNode::Macro(InlineMacro::CrossReference(owner))] = parsed.inlines() else {
        return Err("xref".into());
    };
    let node = CrossReferenceRef::from(owner.as_ref());
    assert_eq!(node.id().ok_or("ID")?.as_str(), "last");
    assert_eq!(node.roles().iter().collect::<Vec<_>>(), ["one", "two"]);
    assert_eq!(node.options().iter().collect::<Vec<_>>(), ["new", "other"]);
    assert!(owner.attributes().get("roles").is_none());
    assert!(owner.attributes().get("xrefstyle").is_none());
    Ok(())
}

#[rstest::rstest]
fn retained_xref_controls_do_not_register_content(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = acdc_parser::parse(
        "xref:target[id=occurrence,title=footnote:ignored[Note],reftext=footnote:other[Other]]\n\n[[target]]\nTarget.",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert!(!parsed.document().references.contains_key("occurrence"));
    assert!(parsed.document().references.contains_key("target"));
    assert_eq!(parsed.document().footnotes, []);
    Ok(())
}

#[rstest::rstest]
fn xref_ids_follow_expansion_includes_and_table_cells(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    use acdc_parser::{
        SafeMode,
        ast::{BlockKind, ContentRef},
        parse_file,
    };

    fn xref(content: ContentRef<'_>) -> Result<CrossReferenceRef<'_>, Error> {
        content
            .inlines()
            .find_map(|inline| {
                if let InlineKind::CrossReference(node) = inline.kind() {
                    Some(node)
                } else {
                    None
                }
            })
            .ok_or_else(|| "xref".into())
    }

    let directory = tempfile::tempdir()?;
    let main = directory.path().join("main.adoc");
    let include = directory.path().join("references.adoc");
    std::fs::write(
        &main,
        ":xref-id: expanded\n\ninclude::references.adoc[]\n\n[cols=a]\n|===\n|xref:target[id=cell]\n|===\n",
    )?;
    let source = "A paragraph.\n\né xref:target[id={xref-id},role=++one two++,opts=++first,second++,custom=++a,b++]\n";
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
    let node = xref(paragraph.content())?;
    let id = node.id().ok_or("included ID")?;
    assert_eq!(id.as_str(), "expanded");
    let resolved = parsed.resolve_location(id.location());
    assert_eq!(resolved.file, Some(include));
    assert_eq!(resolved.location.start.line, 3);
    assert_eq!(
        resolved.location.absolute_start,
        source.find("{xref-id}").ok_or("ID source")?
    );
    assert_eq!(node.roles().iter().collect::<Vec<_>>(), ["one", "two"]);
    assert_eq!(
        node.options().iter().collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert_eq!(
        node.attributes().get("custom"),
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
    let id = xref(paragraph.content())?.id().ok_or("cell ID")?;
    assert_eq!(id.as_str(), "cell");
    assert_eq!(id.location().start().line(), 7);
    Ok(())
}
