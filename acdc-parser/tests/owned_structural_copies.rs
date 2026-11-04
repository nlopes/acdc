use acdc_parser::{
    Block, CompatibilityMode, Document, DocumentAttributes, Location, Options, SafeMode,
    TableFrame,
    ast::{BlockKind, BlockRef, DocumentRef, StructuralRef},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

const SOURCE: &str = "= Copies: Subtitle\nÉlodie Marie Durand <elodie@example.org>\nv2.1, 2026-11-04: Reviewed\n:sectnums:\n:table-frame: sides\n\n[#section,reftext='*Chosen*']\n== Heading\n\nText footnote:[Note].\n\n[#figure]\n.Figure\nimage::bird.png[alt='*Bird*']\n\n.Table\n|===\n|Cell\n|===\n\n:table-frame: ends\n\n|===\n|Later\n|===\n";

#[rstest::rstest]
fn document_copies_keep_catalogs_and_allow_independent_edits(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let original = parsed.document_ref();
    let before = serde_json::to_value(original)?;
    let mut copy = Document::from(original);
    assert_eq!(serde_json::to_value(&copy)?, before);
    assert_eq!(copy.footnotes.len(), 1);
    assert_eq!(copy.toc_entries.len(), 1);
    assert!(copy.references.contains_key("section"));
    assert!(copy.references.contains_key("figure"));
    copy.header = None;
    copy.attributes = DocumentAttributes::default();
    copy.blocks.clear();
    copy.footnotes.clear();
    copy.references.clear();
    copy.toc_entries.clear();
    copy.location = Location::default();
    assert_eq!(serde_json::to_value(original)?, before);
    assert_eq!(original.footnotes().count(), 1);
    assert_eq!(original.toc_entries().count(), 1);
    assert!(original.references().contains("figure"));
    Ok(())
}

#[rstest::rstest]
fn block_copies_keep_source_position_table_defaults_and_edit_isolation(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let tables = parsed.document_ref().descendants().filter_map(|node| {
        if let StructuralRef::Block(block) = node
            && matches!(block.kind(), BlockKind::Table(_))
        {
            Some(block)
        } else {
            None
        }
    });
    let mut frames = Vec::new();
    for block in tables {
        let BlockKind::Table(original) = block.kind() else {
            return Err("table".into());
        };
        let stored = original.presentation().ok_or("stored presentation")?;
        let Block::DelimitedBlock(mut copy) = Block::from(block) else {
            return Err("copy".into());
        };
        let BlockKind::Table(copied) = BlockRef::from(&copy).kind() else {
            return Err("copied table".into());
        };
        assert_eq!(copied.presentation(), Some(stored));
        assert_eq!(
            Location::from(copied.location()),
            Location::from(original.location())
        );
        assert_eq!(
            copied.open_delimiter_location(),
            original.open_delimiter_location()
        );
        assert_eq!(
            copied.close_delimiter_location(),
            original.close_delimiter_location()
        );
        let mut metadata = copy.metadata.clone();
        metadata.attributes.set("frame".into(), "none".into());
        copy.set_metadata(metadata)?;
        let BlockKind::Table(copied) = BlockRef::from(&copy).kind() else {
            return Err("edited table".into());
        };
        assert_eq!(
            copied.presentation().ok_or("edited presentation")?.frame(),
            TableFrame::None
        );
        assert_eq!(original.presentation(), Some(stored));
        frames.push(stored.frame());
    }
    assert_eq!(frames, [TableFrame::Sides, TableFrame::Ends]);
    Ok(())
}

#[rstest::rstest]
fn copies_preserve_selected_include_source_and_declaration_ranges(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("root.adoc");
    let included = directory.path().join("part.adoc");
    std::fs::write(&root, "include::part.adoc[tag=body,indent=0]\n")?;
    std::fs::write(
        &included,
        "// tag::body[]\n    [source,rust]\n    ----\n    λ <1>\n    ----\n// end::body[]\n",
    )?;
    let parsed = parse_file(
        &root,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let block = parsed.document_ref().blocks().next().ok_or("listing")?;
    let BlockKind::Listing(original) = block.kind() else {
        return Err("listing".into());
    };
    let copy = Block::from(block);
    let BlockKind::Listing(copied) = BlockRef::from(&copy).kind() else {
        return Err("copied listing".into());
    };
    assert_eq!(copied.source_text(), original.source_text());
    assert_eq!(
        copied.open_delimiter_location(),
        original.open_delimiter_location()
    );
    assert_eq!(
        copied.close_delimiter_location(),
        original.close_delimiter_location()
    );
    assert_eq!(
        parsed.resolve_location(copied.location()),
        parsed.resolve_location(original.location())
    );
    assert_eq!(
        parsed.resolve_location(copied.location()).file,
        Some(included)
    );
    let document = Document::from(parsed.document_ref());
    assert_eq!(
        serde_json::to_value(DocumentRef::from(&document))?,
        serde_json::to_value(parsed.document_ref())?
    );
    Ok(())
}
