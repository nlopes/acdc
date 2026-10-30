use acdc_parser::{
    CompatibilityMode, IndexTerm, IndexTermKind, InlineMacro, InlineNode, Location, Options,
    ast::{AttributeValueRef, IndexTermRef},
    parse, parse_inline,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn index_metadata_has_one_owner_and_consistent_reads(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "é indexterm2:[Visible,see-also=\"One,Two\",id=entry,role=\"one two\",opts=\"a,b\",custom=kept,title=Tip]";
    let parsed = parse_inline(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [_, inline @ InlineNode::Macro(InlineMacro::IndexTerm(owner))] = parsed.inlines() else {
        return Err("index term".into());
    };
    let node = IndexTermRef::from(owner.as_ref());
    assert_eq!(node.id().ok_or("ID")?.as_str(), "entry");
    assert_eq!(
        node.id().ok_or("ID")?.location().absolute_start(),
        source.find("entry").ok_or("ID source")?
    );
    assert_eq!(node.roles().iter().collect::<Vec<_>>(), ["one", "two"]);
    assert_eq!(node.options().iter().collect::<Vec<_>>(), ["a", "b"]);
    let attributes = node.attributes();
    assert_eq!(
        attributes.get("custom"),
        Some(AttributeValueRef::Text("kept"))
    );
    assert_eq!(
        attributes.get("title"),
        Some(AttributeValueRef::Text("Tip"))
    );
    assert_eq!(attributes.get("role"), attributes.get("roles"));
    assert_eq!(attributes.get("opts"), attributes.get("options"));
    let Some(AttributeValueRef::ContentList(terms)) = attributes.get("terms") else {
        return Err("hierarchy".into());
    };
    assert_eq!(terms.iter().next(), Some(node.term()));
    assert_eq!(terms.iter().count(), 1);
    let Some(AttributeValueRef::ContentList(targets)) = attributes.get("see-also") else {
        return Err("see-also".into());
    };
    assert_eq!(targets.iter().count(), 2);
    assert!(!attributes.contains("see"));
    for (name, value) in attributes.iter() {
        assert_eq!(attributes.get(name), Some(value));
        assert_eq!(attributes.iter().filter(|(key, _)| *key == name).count(), 1);
    }
    assert_eq!(serde_json::to_value(inline)?, serde_json::to_value(owner)?);
    for name in [
        "id", "role", "roles", "opts", "options", "terms", "see", "see-also",
    ] {
        assert!(owner.attributes().get(name).is_none(), "{name}");
    }
    Ok(())
}

#[test]
fn index_control_edits_cannot_shadow_labels_or_metadata() -> Result<(), Error> {
    let mut owner = IndexTerm::new(IndexTermKind::Flow(Vec::new()), Location::default());
    for name in [
        "id", "role", "roles", "opts", "options", "terms", "see", "see-also",
    ] {
        assert!(
            owner.set_attribute(name.into(), "shadow".into()).is_err(),
            "{name}"
        );
        assert!(owner.remove_attribute(name).is_err(), "{name}");
    }
    owner.set_attribute("custom".into(), "".into())?;
    owner.metadata.roles.push("entry");
    owner.options_mut().push("retained");
    let node = IndexTermRef::from(&owner);
    assert_eq!(
        node.attributes().get("custom"),
        Some(AttributeValueRef::Text(""))
    );
    assert_eq!(node.roles().iter().collect::<Vec<_>>(), ["entry"]);
    assert_eq!(node.options().iter().collect::<Vec<_>>(), ["retained"]);
    let Some(AttributeValueRef::ContentList(terms)) = node.attributes().get("terms") else {
        return Err("primary level".into());
    };
    assert!(!terms.is_empty());
    assert!(terms.iter().next().ok_or("empty primary")?.is_empty());
    owner.remove_attribute("custom")?;
    assert!(!IndexTermRef::from(&owner).attributes().contains("custom"));
    Ok(())
}

#[test]
fn index_controls_restore_protected_values_without_registering_content() -> Result<(), Error> {
    let parsed = parse(
        "indexterm2:[Visible,id=\"\",roles=+one two+,options=+a,b+,custom=+a,b]+,title=\"\"]",
        &Options::default(),
    )?;
    let [acdc_parser::Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
        return Err("paragraph".into());
    };
    let [InlineNode::Macro(InlineMacro::IndexTerm(owner))] = paragraph.content.as_slice() else {
        return Err("index term".into());
    };
    let node = IndexTermRef::from(owner.as_ref());
    assert_eq!(node.id().ok_or("present empty ID")?.as_str(), "");
    assert_eq!(node.roles().iter().collect::<Vec<_>>(), ["one", "two"]);
    assert_eq!(node.options().iter().collect::<Vec<_>>(), ["a", "b"]);
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
fn index_controls_freeze_at_macro_recognition_without_duplicating_catalog_metadata()
-> Result<(), Error> {
    let parsed = parse(
        ":label: Expanded\n:control: replaced\n\n[subs=\"specialcharacters,macros,attributes\"]\nindexterm2:[{label},custom={control},id=entry]",
        &Options::default(),
    )?;
    let [acdc_parser::Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
        return Err("paragraph".into());
    };
    let [inline @ InlineNode::Macro(InlineMacro::IndexTerm(owner))] = paragraph.content.as_slice()
    else {
        return Err("index term".into());
    };
    assert_eq!(
        IndexTermRef::from(owner.as_ref())
            .attributes()
            .get("custom"),
        Some(AttributeValueRef::Text("{control}"))
    );
    let json = serde_json::to_value(inline)?;
    assert!(json.get("metadata").is_some());
    assert!(json.get("catalog").is_some());
    assert!(json.pointer("/catalog/metadata").is_none());
    Ok(())
}

#[rstest::rstest]
fn index_ids_follow_expansion_includes_and_table_cells(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    use acdc_parser::{
        SafeMode,
        ast::{BlockKind, ContentRef, InlineKind},
        parse_file,
    };

    fn term(content: ContentRef<'_>) -> Result<IndexTermRef<'_>, Error> {
        content
            .inlines()
            .find_map(|inline| {
                if let InlineKind::IndexTerm(node) = inline.kind() {
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
        ":index-id: expanded\n\ninclude::references.adoc[]\n\n[cols=a]\n|===\n|indexterm2:[Visible,id=cell]\n|===\n",
    )?;
    let source = "A paragraph.\n\né indexterm2:[Visible,id={index-id},role=++one two++,opts=++first,second++,custom=++a,b++]\n";
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
    let node = term(paragraph.content())?;
    let id = node.id().ok_or("included ID")?;
    assert_eq!(id.as_str(), "expanded");
    let resolved = parsed.resolve_location(id.location());
    assert_eq!(resolved.file, Some(include));
    assert_eq!(resolved.location.start.line, 3);
    assert_eq!(
        resolved.location.absolute_start,
        source.find("{index-id}").ok_or("ID source")?
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
    let id = term(paragraph.content())?.id().ok_or("cell ID")?;
    assert_eq!(id.as_str(), "cell");
    assert_eq!(id.location().start().line(), 7);
    Ok(())
}

#[rstest::rstest]
#[case(
    r#"é indexterm2:["Before \"café\" after",custom=kept]"#,
    r#"Before "café" after"#,
    r#"Before \"café\" after"#
)]
#[case(
    r"é indexterm2:['Before \'café\' after',custom=kept]",
    "Before 'café' after",
    r"Before \'café\' after"
)]
fn index_quoted_labels_keep_authored_escape_ranges(
    #[case] source: &str,
    #[case] expected: &str,
    #[case] authored: &str,
) -> Result<(), Error> {
    let parsed = parse_inline(source, &Options::default())?;
    let [_, InlineNode::Macro(InlineMacro::IndexTerm(owner))] = parsed.inlines() else {
        return Err("index term".into());
    };
    let [InlineNode::PlainText(label)] = owner.term() else {
        return Err(format!("label: {:?}", owner.term()).into());
    };
    assert_eq!(label.content, expected);
    assert_eq!(
        &source[label.location.absolute_start..=label.location.absolute_end],
        authored
    );
    Ok(())
}

#[rstest::rstest]
fn index_unquoted_none_controls_are_ignored(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse_inline(
        "indexterm2:[Visible,custom=before,custom=None,removed=None,title=\"None\",see=None,see-also=Target]",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [InlineNode::Macro(InlineMacro::IndexTerm(owner))] = parsed.inlines() else {
        return Err("index term".into());
    };
    let attributes = IndexTermRef::from(owner.as_ref()).attributes();
    assert_eq!(
        attributes.get("custom"),
        Some(AttributeValueRef::Text("before"))
    );
    assert_eq!(
        attributes.get("title"),
        Some(AttributeValueRef::Text("None"))
    );
    assert!(!attributes.contains("removed"));
    assert!(!attributes.contains("see"));
    assert!(attributes.contains("see-also"));
    Ok(())
}
