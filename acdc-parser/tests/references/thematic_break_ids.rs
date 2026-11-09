use acdc_parser::{
    Anchor, Block, CompatibilityMode, ContentAttribute, Document, Location, Options, Plain,
    ThematicBreak, WarningKind,
    ast::{AnchorRef, BlockRef, ContentRef, DocumentRef, InlineKind, ReferenceTargetRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case::single("[[second,Selected]]", &["second"], Some("Selected"))]
#[case::stacked("[[first,Earlier]]\n[[second,Selected]]", &["first", "second"], Some("Selected"))]
#[case::named("[id=second,reftext='Selected']", &["second"], Some("Selected"))]
#[case::named_first("[id=first,reftext='Earlier']\n[[second,Selected]]", &["second"], Some("Selected"))]
#[case::named_last("[[first,Earlier]]\n[id=second,reftext='Selected']", &["first", "second"], Some("Selected"))]
#[case::inherit_anchor("[[first,Selected]]\n[[second]]", &["first", "second"], Some("Selected"))]
#[case::inherit_named("[id=first,reftext='Selected']\n[[second]]", &["second"], Some("Selected"))]
#[case::label_before("[reftext='Selected']\n[[second]]", &["second"], Some("Selected"))]
#[case::label_after("[[second]]\n[reftext='Selected']", &["second"], Some("Selected"))]
#[case::empty_label("[[second,Earlier]]\n[reftext='']", &["second"], Some(""))]
#[case::no_label("[[first]]\n[[second]]", &["first", "second"], None)]
fn source_ids_and_labels_survive_reads_exports_and_repair(
    #[case] metadata: &str,
    #[case] declarations: &[&str],
    #[case] label: Option<&str>,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("{metadata}\n'''\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let read = parsed.document();
    let before = serde_json::to_value(read)?;
    let block = read.blocks().next().ok_or("thematic break")?;
    assert_eq!(block.id().ok_or("primary ID")?.as_str(), "second");
    let primary = block.primary_anchor().ok_or("primary anchor")?;
    assert_eq!(primary.id(), "second");
    assert_eq!(label_text(primary.reftext()).as_deref(), label);
    assert_eq!(
        block.anchors().map(AnchorRef::id).collect::<Vec<_>>(),
        declarations
    );
    let location = primary.location();
    assert!(
        source
            .get(location.absolute_start()..=location.absolute_end())
            .ok_or("ID range")?
            .contains("second")
    );
    assert_catalog(read, label)?;
    assert_eq!(parsed.warnings(), []);
    #[cfg(feature = "asg")]
    {
        let value =
            acdc_parser::asg::export_document_with_profile(read, acdc_parser::asg::Profile::Acdc)?;
        assert_eq!(
            value
                .pointer("/blocks/0/id")
                .and_then(serde_json::Value::as_str),
            Some("second")
        );
        assert!(value.pointer("/blocks/0/id_location").is_some());
        assert!(value.pointer("/references/second").is_some());
        assert!(value.pointer("/references/first").is_none());
    }
    assert_eq!(serde_json::to_value(read)?, before);
    let mut prepared = Document::from(read);
    prepared.rebuild_references_and_toc()?;
    assert_catalog(DocumentRef::from(&prepared), label)?;
    assert_eq!(serde_json::to_value(DocumentRef::from(&prepared))?, before);
    Ok(())
}

fn assert_catalog(document: DocumentRef<'_>, label: Option<&str>) -> Result<(), Error> {
    assert!(!document.references().contains("first"));
    let reference = document.references().get("second").ok_or("registered ID")?;
    assert_eq!(reference.target(), "second");
    assert_eq!(
        label_text(reference.xreflabel()).as_deref(),
        label.filter(|value| !value.is_empty())
    );
    Ok(())
}

#[rstest::rstest]
fn named_labels_resolve_natural_references(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[id=second,reftext='Selected label']\n'''\n\n<<Selected label>>.\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.warnings(), []);
    assert_eq!(
        label_text(
            parsed
                .document()
                .references()
                .get("second")
                .and_then(ReferenceTargetRef::xreflabel)
        )
        .as_deref(),
        Some("Selected label")
    );
    Ok(())
}

#[rstest::rstest]
fn unused_declarations_do_not_reserve_another_blocks_id(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[[shared,Unused]]\n[[second,Rule label]]\n'''\n\n[[shared,Paragraph label]]\nParagraph.\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let read = parsed.document();
    assert_eq!(parsed.warnings(), []);
    assert_eq!(
        label_text(
            read.references()
                .get("shared")
                .and_then(ReferenceTargetRef::xreflabel)
        )
        .as_deref(),
        Some("Paragraph label")
    );
    #[cfg(feature = "asg")]
    {
        let value =
            acdc_parser::asg::export_document_with_profile(read, acdc_parser::asg::Profile::Acdc)?;
        assert_eq!(
            value
                .pointer("/references/shared/owner")
                .and_then(serde_json::Value::as_str),
            Some("/blocks/1")
        );
        assert_eq!(
            value
                .pointer("/blocks/0/anchors/0/id")
                .and_then(serde_json::Value::as_str),
            Some("shared")
        );
    }
    Ok(())
}

#[rstest::rstest]
fn duplicate_primary_ids_keep_the_first_label(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[[unused]]\n[[shared,First label]]\n'''\n\n[[shared,Second label]]\n'''\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(
        label_text(
            parsed
                .document()
                .references()
                .get("shared")
                .and_then(ReferenceTargetRef::xreflabel)
        )
        .as_deref(),
        Some("First label")
    );
    assert!(!parsed.document().references().contains("unused"));
    assert_eq!(parsed.warnings().iter().filter(|warning| matches!(&warning.kind, WarningKind::DuplicateId { id, .. } if id == "shared")).count(), 1);
    Ok(())
}

#[test]
fn prepared_selection_does_not_invent_registration_or_source_locations() -> Result<(), Error> {
    let mut rule = ThematicBreak::new(Location::default());
    rule.set_anchors([
        Anchor::new("first", Location::default()),
        Anchor::new("second", Location::default()).with_reftext(Some(ContentAttribute::new(vec![
            acdc_parser::InlineNode::PlainText(Plain::new("Selected", Location::default())),
        ]))),
    ]);
    let mut document = Document::default();
    document.push_block(Block::ThematicBreak(rule));
    let read = DocumentRef::from(&document);
    assert_eq!(
        read.blocks()
            .next()
            .and_then(BlockRef::id)
            .ok_or("prepared ID")?
            .as_str(),
        "second"
    );
    assert!(read.references().is_empty());
    document.rebuild_references_and_toc()?;
    let read = DocumentRef::from(&document);
    assert_catalog(read, Some("Selected"))?;
    #[cfg(feature = "asg")]
    {
        let value =
            acdc_parser::asg::export_document_with_profile(read, acdc_parser::asg::Profile::Acdc)?;
        assert!(value.pointer("/blocks/0/id_location").is_none());
        assert!(value.pointer("/blocks/0/anchors/0/location").is_none());
    }
    Ok(())
}

#[rstest::rstest]
fn selected_label_references_receive_the_normal_mode_diagnostics(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "[id=target,reftext='Label <<missing>>']\n'''\n\nSee <<target>>.\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let missing = parsed.warnings().iter().filter(|warning| {
        matches!(&warning.kind, WarningKind::UnresolvedReference { target, .. } if target == "missing")
    }).count();
    assert_eq!(missing, usize::from(mode == CompatibilityMode::Draft));
    Ok(())
}

fn label_text(label: Option<ContentRef<'_>>) -> Option<String> {
    label.map(|label| {
        label
            .inlines()
            .filter_map(|inline| {
                if let InlineKind::Plain(value) = inline.kind() {
                    Some(value.text())
                } else if let InlineKind::Raw(value) = inline.kind() {
                    Some(value.text())
                } else {
                    None
                }
            })
            .collect()
    })
}
