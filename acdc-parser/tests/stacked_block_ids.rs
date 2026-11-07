use acdc_parser::{
    Anchor, Block, BlockMetadata, CompatibilityMode, Document, Location, Options, Paragraph,
    WarningKind,
    ast::{AnchorRef, BlockRef, ContentRef, DocumentRef, InlineKind, ReferenceRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case::anchors("[[first,Earlier]]\n[[second,Selected]]\n", true)]
#[case::retained_label("[[first,Selected]]\n[[second]]\n", true)]
#[case::named_first("[id=first,reftext=\"Earlier\"]\n[[second,Selected]]\n", false)]
#[case::named_last("[[first,Earlier]]\n[id=second,reftext=\"Selected\"]\n", true)]
#[case::named_ids("[id=first]\n[id=second,reftext=\"Selected\"]\n", false)]
fn the_last_source_id_selects_the_registered_block(
    #[case] metadata: &str,
    #[case] retains_first: bool,
    #[values("Paragraph.", "== Heading\n\nBody.", "----\nCode.\n----", "* Item")] body: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("= Document\n\n{metadata}{body}\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let document = parsed.document();
    let before = serde_json::to_value(document)?;
    let block = document.blocks().next().ok_or("block")?;
    assert_eq!(block.id().ok_or("selected ID")?.as_str(), "second");
    let selected = block.primary_anchor().ok_or("selected anchor")?;
    assert_eq!(selected.id(), "second");
    let location = selected.location();
    assert!(
        source
            .get(location.absolute_start()..=location.absolute_end())
            .ok_or("selected source range")?
            .contains("second")
    );
    let mut declarations: Vec<_> = block.anchors().map(AnchorRef::id).collect();
    declarations.sort_unstable();
    assert_eq!(
        declarations,
        if retains_first {
            vec!["first", "second"]
        } else {
            vec!["second"]
        }
    );
    assert!(!document.references().contains("first"));
    let reference = document
        .references()
        .get("second")
        .ok_or("selected registration")?;
    assert_eq!(reference.target(), "second");
    assert_eq!(
        label_text(reference.xreflabel().ok_or("label")?),
        "Selected"
    );
    assert_eq!(parsed.warnings(), []);
    assert_eq!(serde_json::to_value(document)?, before);
    #[cfg(feature = "asg")]
    {
        let value =
            acdc_parser::asg::document_with_profile(document, acdc_parser::asg::Profile::Acdc)?;
        assert_eq!(
            value
                .pointer("/blocks/0/id")
                .and_then(serde_json::Value::as_str),
            Some("second")
        );
        assert_eq!(
            value
                .pointer("/blocks/0/anchors/0/id")
                .and_then(serde_json::Value::as_str),
            retains_first.then_some("first")
        );
        assert!(value.pointer("/blocks/0/id_location").is_some());
        assert_eq!(serde_json::to_value(document)?, before);
    }
    let mut prepared = Document::from(document);
    prepared.rebuild_references_and_toc()?;
    let repaired = DocumentRef::from(&prepared);
    assert!(repaired.references().contains("second"));
    assert!(!repaired.references().contains("first"));
    assert_eq!(
        label_text(
            repaired
                .references()
                .get("second")
                .and_then(ReferenceRef::xreflabel)
                .ok_or("repaired label")?
        ),
        "Selected"
    );
    assert_eq!(
        repaired
            .blocks()
            .next()
            .and_then(BlockRef::primary_anchor)
            .map(AnchorRef::id),
        Some("second")
    );
    Ok(())
}

#[test]
fn prepared_ids_select_one_registration_without_inventing_source_spans() -> Result<(), Error> {
    for explicit in [None, Some("selected")] {
        let mut metadata = BlockMetadata::default();
        metadata.anchors = vec![
            Anchor::new("first", Location::default()),
            Anchor::new("second", Location::default()),
        ];
        metadata.id = explicit.map(|id| Anchor::new(id, Location::default()));
        let mut paragraph = Paragraph::new(Vec::new(), Location::default());
        paragraph.set_metadata(metadata)?;
        let mut document = Document::default();
        document.push_block(Block::Paragraph(paragraph));
        let selected = explicit.unwrap_or("second");
        let before = serde_json::to_value(DocumentRef::from(&document))?;
        assert_eq!(
            DocumentRef::from(&document)
                .blocks()
                .next()
                .and_then(BlockRef::primary_anchor)
                .map(AnchorRef::id),
            Some(selected)
        );
        assert!(DocumentRef::from(&document).references().is_empty());
        assert_eq!(serde_json::to_value(DocumentRef::from(&document))?, before);
        document.rebuild_references_and_toc()?;
        let read = DocumentRef::from(&document);
        for id in ["first", "second", "selected"] {
            assert_eq!(read.references().contains(id), id == selected);
        }
        #[cfg(feature = "asg")]
        {
            let value =
                acdc_parser::asg::document_with_profile(read, acdc_parser::asg::Profile::Acdc)?;
            assert_eq!(
                value
                    .pointer("/blocks/0/id")
                    .and_then(serde_json::Value::as_str),
                Some(selected)
            );
            assert!(value.pointer("/blocks/0/id_location").is_none());
            for anchor in value
                .pointer("/blocks/0/anchors")
                .and_then(serde_json::Value::as_array)
                .ok_or("declarations")?
            {
                assert!(anchor.get("location").is_none());
            }
        }
    }
    Ok(())
}

#[rstest::rstest]
fn duplicate_selected_ids_keep_the_first_registration(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[[unused-first]]\n[[shared,First label]]\nFirst.\n\n[[unused-second]]\n[[shared,Second label]]\nSecond.\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let references = parsed.document().references();
    assert!(!references.contains("unused-first"));
    assert!(!references.contains("unused-second"));
    let label = references
        .get("shared")
        .and_then(ReferenceRef::xreflabel)
        .ok_or("first label")?;
    assert_eq!(label_text(label), "First label");
    assert_eq!(parsed.warnings().iter().filter(|warning| matches!(&warning.kind, WarningKind::DuplicateId { id, .. } if id == "shared")).count(), 1);
    Ok(())
}

fn label_text(label: ContentRef<'_>) -> String {
    label
        .inlines()
        .filter_map(|inline| {
            if let InlineKind::Plain(plain) = inline.kind() {
                Some(plain.text())
            } else if let InlineKind::Raw(raw) = inline.kind() {
                Some(raw.text())
            } else {
                None
            }
        })
        .collect()
}
