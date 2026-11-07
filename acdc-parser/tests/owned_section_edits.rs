//! Section edits preserve assigned identity until its authored selector changes.

use acdc_parser::{
    Anchor, Block, CompatibilityMode, Document, Location, Options, Section, SectionKind, Title,
    ast::{BlockKind, ContentRef, DocumentRef, IdOrigin, SectionRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

const SOURCE: &str =
    "= Guide\n:sectnums:\n:toc:\n\n== Same\n\nText.\n\n== Same\n\nNested *body*.\n";

fn section(document: DocumentRef<'_>, index: usize) -> Result<Section<'_>, Error> {
    let Block::Section(section) = Block::from(document.blocks().nth(index).ok_or("section")?)
    else {
        return Err("section".into());
    };
    Ok(section)
}

#[rstest::rstest]
fn transfers_preserve_the_assigned_collision_id_and_prepared_values(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut edited = section(parsed.document(), 1)?;
    let original = edited.clone();
    let metadata = edited.take_metadata();
    let title = edited.take_title();
    let blocks = edited.take_blocks();
    let read = SectionRef::from(&edited);
    assert!(read.title().is_none());
    assert_eq!(read.blocks().count(), 0);
    assert_eq!(read.id().ok_or("assigned ID")?.as_str(), "_same_2");
    edited.set_metadata(metadata)?;
    edited.set_title(title);
    edited.set_blocks(blocks);
    assert_eq!(edited, original);
    assert_eq!(
        serde_json::to_value(&edited)?,
        serde_json::to_value(&original)?
    );
    Ok(())
}

#[rstest::rstest]
fn semantic_edits_retain_assigned_ids_and_leave_catalogs_unchanged(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let edited = section(parsed.document(), 1)?;
    let mut metadata = edited.clone_metadata();
    metadata.roles.push("edited");
    metadata.attributes.set("custom".into(), "value".into());
    let mut edited = edited.with_metadata(metadata)?;
    edited.set_title(Title::new(Vec::new()));
    edited.set_level(3);
    edited.set_location(Location::default());
    let read = SectionRef::from(&edited);
    assert_eq!(read.id().ok_or("assigned ID")?.as_str(), "_same_2");
    assert_eq!(
        read.id().ok_or("assigned ID")?.origin(),
        IdOrigin::Generated
    );
    assert!(read.title().is_some_and(ContentRef::is_empty));
    assert!(read.roles().contains("edited"));
    assert_eq!(read.level(), 3);
    assert_eq!(read.number(), Some("2"));
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    assert!(parsed.document().references().contains("_same_2"));
    Ok(())
}

#[rstest::rstest]
fn changing_and_removing_authored_identity_invalidates_only_the_local_assignment(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut edited = section(parsed.document(), 1)?;
    let mut metadata = edited.clone_metadata();
    metadata.id = Some(Anchor::new("authored", Location::default()));
    edited.set_metadata(metadata)?;
    let read = SectionRef::from(&edited);
    assert_eq!(read.id().ok_or("authored ID")?.as_str(), "authored");
    assert_eq!(read.id().ok_or("authored ID")?.origin(), IdOrigin::Explicit);
    let mut metadata = edited.take_metadata();
    assert!(SectionRef::from(&edited).id().is_none());
    metadata.id = None;
    edited.set_metadata(metadata)?;
    assert!(SectionRef::from(&edited).id().is_none());
    assert_eq!(SectionRef::from(&edited).resolve_id(), "_same");
    assert!(parsed.document().references().contains("_same_2"));
    assert!(!parsed.document().references().contains("authored"));
    Ok(())
}

#[rstest::rstest]
fn failed_metadata_replacements_leave_all_section_state_unchanged(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut edited = section(parsed.document(), 1)?;
    let original = edited.clone();
    for name in [
        "title",
        "id",
        "roles",
        "opts",
        "style",
        "caption",
        "reftext",
        "subs",
        "attribution",
        "citetitle",
    ] {
        let mut metadata = edited.clone_metadata();
        metadata.style = Some("appendix");
        metadata.id = Some(Anchor::new("rejected", Location::default()));
        metadata.attributes.set(name.into(), "shadow".into());
        assert!(edited.set_metadata(metadata).is_err(), "{name}");
        assert_eq!(edited, original, "{name}");
    }
    Ok(())
}

#[test]
fn prepared_styles_and_numbering_follow_explicit_edits() -> Result<(), Error> {
    let parsed = parse(SOURCE, &Options::default())?;
    let mut edited = section(parsed.document(), 1)?;
    let mut metadata = edited.clone_metadata();
    metadata.style = SectionKind::Appendix.as_style();
    edited.set_metadata(metadata)?;
    let read = SectionRef::from(&edited);
    assert_eq!(read.kind(), SectionKind::Appendix);
    assert_eq!(read.style(), Some("appendix"));
    assert_eq!(read.number(), Some("2"));
    edited.set_numbering(true);
    assert!(SectionRef::from(&edited).number().is_none());
    let mut document = Document::default();
    document.push_block(Block::Section(edited));
    document.renumber_sections();
    let BlockKind::Section(read) = DocumentRef::from(&document)
        .blocks()
        .next()
        .ok_or("section")?
        .kind()
    else {
        return Err("section".into());
    };
    assert_eq!(read.number(), Some("A"));
    let mut edited = section(DocumentRef::from(&document), 0)?;
    let mut metadata = edited.take_metadata();
    assert_eq!(SectionRef::from(&edited).kind(), SectionKind::Normal);
    metadata.style = Some("custom");
    edited.set_metadata(metadata)?;
    assert_eq!(SectionRef::from(&edited).kind(), SectionKind::Normal);
    assert_eq!(SectionRef::from(&edited).style(), Some("custom"));
    Ok(())
}

#[rstest::rstest]
fn owned_children_outlive_the_old_owner_and_keep_source_provenance(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let original = section(parsed.document(), 1)?;
    let blocks = {
        let mut local = original.clone();
        let blocks = local.take_blocks();
        drop(local);
        blocks
    };
    let mut rebuilt = original.clone();
    rebuilt.set_blocks(blocks);
    assert_eq!(rebuilt, original);
    let block = SectionRef::from(&rebuilt).blocks().next().ok_or("body")?;
    let BlockKind::Paragraph(paragraph) = block.kind() else {
        return Err("paragraph".into());
    };
    assert_eq!(paragraph.source_text(), Some("Nested *body*."));
    Ok(())
}
