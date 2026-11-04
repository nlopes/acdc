//! Detached edits keep source declarations and catalogs until an explicit update.

use acdc_parser::{
    Author, Block, CompatibilityMode, Document, Options, Reference, Title, TocEntry,
    ast::{
        BlockKind, BlockRef, CaptionRef, ContentRef, DocumentRef, InlineKind, ReferenceRef,
        SectionRef, TocEntryRef,
    },
    parse,
};

type Error = Box<dyn std::error::Error>;

fn text(content: ContentRef<'_>) -> Result<String, Error> {
    content.inlines().try_fold(String::new(), |mut text, node| {
        let InlineKind::Plain(plain) = node.kind() else {
            return Err("expected plain test content".into());
        };
        text.push_str(plain.text());
        Ok(text)
    })
}

#[rstest::rstest]
fn removing_content_and_renumbering_do_not_rebuild_catalog_membership(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Catalogs\n:sectnums:\n\n[[first]]\n== First\n\nText footnote:[Note].\n\n[[second]]\n== Second\n\nBody.\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let mut document = Document::from(parsed.document());
    document.blocks.remove(0);
    document.renumber_sections();

    let read = DocumentRef::from(&document);
    let BlockKind::Section(second) = read.blocks().next().ok_or("remaining section")?.kind() else {
        return Err("section".into());
    };
    assert_eq!(second.number(), Some("1"));
    assert_eq!(
        read.references()
            .get("second")
            .ok_or("second")?
            .section_number(),
        Some("1")
    );
    assert!(read.references().contains("first"));
    assert_eq!(read.toc_entries().count(), 2);
    assert_eq!(read.footnotes().count(), 1);
    assert_eq!(read.blocks().count(), 1);
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[rstest::rstest]
fn caption_renumbering_keeps_declaration_prefixes_and_reference_snapshots(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Captions\n:figure-caption: First\n\n[#one]\n.One\nimage::one.png[]\n\n:figure-caption: AtSecond\n\n[#two]\n.Two\nimage::two.png[]\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut document = Document::from(parsed.document());
    document.blocks.retain(|block| {
        BlockRef::from(block)
            .id()
            .is_none_or(|id| id.as_str() != "one")
    });
    document.attributes =
        Options::with_attributes([("figure-caption", "Now")])?.into_document_attributes();
    document.renumber_captions();
    let read = DocumentRef::from(&document);
    let image = read
        .blocks()
        .find_map(|block| {
            if let BlockKind::Image(image) = block.kind() {
                Some(image)
            } else {
                None
            }
        })
        .ok_or("remaining image")?;
    let caption =
        CaptionRef::resolve(image.attributes(), read.attributes(), None).ok_or("caption")?;
    assert!(
        matches!(caption, CaptionRef::Numbered { label: "AtSecond", number: Some(number), .. } if number.get() == 1)
    );
    assert!(
        matches!(read.references().get("two").ok_or("two")?.caption(), Some(CaptionRef::Numbered { label: "AtSecond", number: Some(number), .. }) if number.get() == 2)
    );
    assert!(read.references().contains("one"));
    Ok(())
}

#[rstest::rstest]
fn author_and_document_attribute_edits_do_not_reinterpret_registered_content(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Registration\nAlice Smith <alice@example.org>\n:label: Original\n\nindexterm:[{label}]\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut document = Document::from(parsed.document());
    document.header.as_mut().ok_or("header")?.authors =
        vec![Author::from_parts("Bob", None, "Jones", "BJ")];
    assert_eq!(
        DocumentRef::from(&document)
            .attributes()
            .get("author")
            .and_then(|value| value.text()),
        Some("Alice Smith")
    );
    document.attributes =
        Options::with_attributes([("label", "Changed")])?.into_document_attributes();
    let read = DocumentRef::from(&document);
    let term = read
        .blocks()
        .find_map(|block| {
            if let BlockKind::Paragraph(paragraph) = block.kind() {
                paragraph.content().inlines().find_map(|node| {
                    if let InlineKind::IndexTerm(term) = node.kind() {
                        Some(term)
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        })
        .ok_or("index occurrence")?;
    assert_eq!(text(term.term())?, "Original");
    assert_eq!(text(term.catalog().term())?, "Original");
    assert_eq!(
        read.header()
            .ok_or("header")?
            .authors()
            .next()
            .ok_or("author")?
            .first_name(),
        "Bob"
    );
    Ok(())
}

#[rstest::rstest]
fn reference_outer_presence_and_toc_title_presence_are_distinct_contracts(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Titles\n\n[[target]]\n== Target\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let reference = parsed
        .document()
        .references()
        .get("target")
        .ok_or("target")?;
    let toc = parsed.document().toc_entries().next().ok_or("toc")?;
    for (title, outer_present, inner_present) in [
        (None, false, None),
        (Some(Title::default()), true, Some(false)),
        (Some(Title::new(Vec::new())), true, Some(true)),
    ] {
        let mut edited = Reference::from(reference);
        edited.title = title;
        let read = ReferenceRef::from(&edited);
        assert_eq!(read.title().is_some(), outer_present);
        assert!(read.title().is_none_or(ContentRef::is_empty));
        let copy = Reference::from(read);
        assert_eq!(copy.title.as_ref().map(Title::is_present), inner_present);
    }
    let mut edited = TocEntry::from(toc);
    edited.title = Title::default();
    assert!(TocEntryRef::from(&edited).title().is_none());
    edited.title = Title::new(Vec::new());
    assert!(
        TocEntryRef::from(&edited)
            .title()
            .is_some_and(ContentRef::is_empty)
    );
    Ok(())
}

#[rstest::rstest]
fn nonidentity_edits_keep_parser_assigned_collision_ids(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Identity\n\n== Same\n\nFirst.\n\n== Same\n\nSecond.\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut document = Document::from(parsed.document());
    let Block::Section(second) = document.blocks.get_mut(1).ok_or("second section")? else {
        return Err("section".into());
    };
    second.metadata.roles.push("highlighted");
    second.title = Title::new(Vec::new());
    let section = SectionRef::from(&*second);
    assert_eq!(section.id().ok_or("assigned ID")?.as_str(), "_same_2");
    assert!(section.title().is_some_and(ContentRef::is_empty));
    assert_eq!(
        text(
            DocumentRef::from(&document)
                .references()
                .get("_same_2")
                .ok_or("registered title")?
                .title()
                .ok_or("title")?
        )?,
        "Same"
    );
    Ok(())
}
