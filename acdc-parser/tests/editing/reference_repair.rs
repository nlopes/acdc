use acdc_parser::{
    Anchor, Block, BlockMetadata, CompatibilityMode, ContentAttribute, Document, InlineNode,
    Location, Options, Plain, ReferenceTarget, SafeMode, Section, Title,
    ast::{BlockKind, BlockRef, ContentRef, DocumentRef, InlineKind, TocEntryRef},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;
const MODES: [CompatibilityMode; 2] = [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft];

fn title(text: &'static str) -> Title<'static> {
    Title::new(vec![InlineNode::PlainText(Plain::new(
        text,
        Location::default(),
    ))])
}

fn text(content: ContentRef<'_>) -> String {
    content
        .inlines()
        .map(|node| {
            if let InlineKind::Plain(plain) = node.kind() {
                plain.text().to_owned()
            } else if let InlineKind::Bold(span) = node.kind() {
                text(span.content())
            } else {
                String::new()
            }
        })
        .collect()
}

fn section(document: DocumentRef<'_>, index: usize) -> Result<Section<'_>, Error> {
    let Block::Section(section) = Block::from(document.blocks().nth(index).ok_or("section")?)
    else {
        return Err("section".into());
    };
    Ok(section)
}

#[test]
fn explicit_rebuild_tracks_insert_remove_replace_and_retains_generated_ids() -> Result<(), Error> {
    for mode in MODES {
        let parsed = parse(
            "= Book\n:sectnums:\n\n== Same\n\nFirst.\n\n== Same\n\nSecond.\n",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let mut document = Document::from(parsed.document());
        let mut retained = section(parsed.document(), 1)?;
        retained.set_title(title("Changed"));
        let mut inserted = retained.clone();
        inserted.set_metadata(
            BlockMetadata::default().with_id(Some(Anchor::new("inserted", Location::default()))),
        )?;
        inserted.set_title(title("Inserted"));
        document.set_blocks([Block::Section(retained), Block::Section(inserted)]);
        assert!(DocumentRef::from(&document).references().contains("_same"));
        document.rebuild_references_and_toc()?;
        let read = DocumentRef::from(&document);
        assert!(!read.references().contains("_same"));
        assert_eq!(
            read.toc_entries().map(TocEntryRef::id).collect::<Vec<_>>(),
            ["_same_2", "inserted"]
        );
        let changed = read.references().get("_same_2").ok_or("retained target")?;
        assert_eq!(changed.title().map(text).as_deref(), Some("Changed"));
        assert_eq!(changed.section_number(), Some("1"));
        assert_eq!(
            read.references()
                .get("inserted")
                .ok_or("new target")?
                .section_number(),
            Some("2")
        );
        let Some(BlockKind::Section(retained)) = read.blocks().next().map(BlockRef::kind) else {
            return Err("section".into());
        };
        assert_eq!(
            retained.id().ok_or("assigned ID")?.origin(),
            acdc_parser::ast::IdOrigin::Generated
        );
    }
    Ok(())
}

#[test]
fn copied_headings_with_equal_source_spans_keep_first_target_and_separate_toc_ownership()
-> Result<(), Error> {
    for mode in MODES {
        let parsed = parse(
            "= Book\n:sectnums:\n\n[#shared]\n== Original\n\nBody.\n",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let first = section(parsed.document(), 0)?;
        let second = first.clone();
        let mut document = Document::from(parsed.document());
        document.set_blocks([Block::Section(first), Block::Section(second)]);
        document.rebuild_references_and_toc()?;
        let read = DocumentRef::from(&document);
        let entries: Vec<_> = read.toc_entries().collect();
        let [first, second] = entries.as_slice() else {
            return Err("two entries".into());
        };
        assert_eq!(first.location(), second.location());
        let reference = read.references().get("shared").ok_or("shared")?;
        assert!(reference.matches_toc_entry(*first));
        assert!(!reference.matches_toc_entry(*second));
        assert_eq!(reference.section_number(), Some("1"));
        assert_eq!(second.number(), Some("2"));
        document.renumber_sections();
        assert_eq!(
            DocumentRef::from(&document)
                .references()
                .get("shared")
                .ok_or("shared")?
                .section_number(),
            Some("1")
        );
    }
    Ok(())
}

#[test]
fn missing_assigned_identity_fails_before_changing_tree_or_catalogs() -> Result<(), Error> {
    let parsed = parse("[#first]\n== First\n", &Options::default())?;
    let mut document = Document::from(parsed.document());
    let mut blocks: Vec<_> = document.take_blocks().collect();
    blocks.push(Block::Section(Section::new(
        title("Unassigned"),
        1,
        Vec::new(),
        Location::default(),
    )));
    document.set_blocks(blocks);
    let before = serde_json::to_value(DocumentRef::from(&document))?;
    let references: std::collections::HashMap<_, _> = document.take_references().collect();
    document.set_references(references.clone());
    let toc: Vec<_> = document.take_toc_entries().collect();
    document.set_toc_entries(toc.clone());
    assert!(document.rebuild_references_and_toc().is_err());
    assert_eq!(serde_json::to_value(DocumentRef::from(&document))?, before);
    assert_eq!(
        std::collections::HashMap::from(DocumentRef::from(&document).references()),
        references
    );
    assert_eq!(
        DocumentRef::from(&document)
            .toc_entries()
            .map(acdc_parser::TocEntry::from)
            .collect::<Vec<_>>(),
        toc
    );
    Ok(())
}

#[test]
fn aliases_follow_surviving_canonical_targets_and_disappear_with_the_target() -> Result<(), Error> {
    let parsed = parse("[#target]\n== Target\n", &Options::default())?;
    let mut document = Document::from(parsed.document());
    let target = ReferenceTarget::from(
        parsed
            .document()
            .references()
            .get("target")
            .ok_or("target")?,
    );
    let mut references: Vec<_> = document.take_references().collect();
    references.push(("alias", target));
    document.set_references(references);
    document.rebuild_references_and_toc()?;
    let read = DocumentRef::from(&document);
    assert_eq!(
        read.references().get("alias").ok_or("alias")?.target(),
        "target"
    );
    assert_eq!(
        read.references().get("alias").ok_or("alias")?.title(),
        read.references().get("target").ok_or("target")?.title()
    );
    document.set_blocks([]);
    document.rebuild_references_and_toc()?;
    assert!(DocumentRef::from(&document).references().is_empty());
    assert_eq!(DocumentRef::from(&document).toc_entries().len(), 0);
    Ok(())
}

#[test]
fn caller_supplied_title_presence_is_preserved_at_each_catalog_boundary() -> Result<(), Error> {
    for value in [Title::default(), Title::new(Vec::new())] {
        let present = value.is_present();
        let section = Section::new(value, 1, Vec::new(), Location::default()).with_metadata(
            BlockMetadata::default().with_id(Some(Anchor::new("empty", Location::default()))),
        )?;
        let mut document = Document::default();
        document.set_blocks([Block::Section(section)]);
        document.rebuild_references_and_toc()?;
        let read = DocumentRef::from(&document);
        let entry = read.toc_entries().next().ok_or("entry")?;
        let reference = read.references().get("empty").ok_or("reference")?;
        assert!(reference.title().is_some_and(ContentRef::is_empty));
        assert_eq!(entry.title().is_some(), present);
        assert_eq!(reference.matches_toc_entry(entry), present);
    }
    Ok(())
}

#[test]
fn repeated_selected_includes_retain_source_locations_and_distinct_ownership() -> Result<(), Error>
{
    for mode in MODES {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("main.adoc");
        std::fs::write(
            directory.path().join("part.adoc"),
            "Discarded.\n\n// tag::selected[]\n[#shared]\n== Included\n\nBody.\n// end::selected[]\n",
        )?;
        std::fs::write(
            &path,
            "= T\n\ninclude::part.adoc[tag=selected]\n\ninclude::part.adoc[tag=selected]\n",
        )?;
        let parsed = parse_file(
            &path,
            &Options::builder()
                .with_safe_mode(SafeMode::Unsafe)
                .with_compatibility_mode(mode)
                .build()?,
        )?;
        let locations: Vec<_> = parsed
            .document()
            .toc_entries()
            .map(|entry| Location::from(entry.location()))
            .collect();
        let mut document = Document::from(parsed.document());
        document.rebuild_references_and_toc()?;
        let read = DocumentRef::from(&document);
        assert_eq!(
            read.toc_entries()
                .map(|entry| Location::from(entry.location()))
                .collect::<Vec<_>>(),
            locations
        );
        let entries: Vec<_> = read.toc_entries().collect();
        let [first, second] = entries.as_slice() else {
            return Err("two entries".into());
        };
        let reference = read.references().get("shared").ok_or("shared")?;
        assert!(reference.matches_toc_entry(*first));
        assert!(!reference.matches_toc_entry(*second));
    }
    Ok(())
}

#[test]
fn rebuild_reuses_selected_content_registration_and_caption_snapshots() -> Result<(), Error> {
    for mode in MODES {
        let parsed = parse(
            "[[document-header,Header label]]\n= T: Details\n\n[#paragraph]\n.P title\nlink:https://example.org[Link,id=link] [#span]*Span* footnote:[anchor:note[]Note] image:bird.svg[Bird,id=image] indexterm2:[Visible anchor:visible[]term] indexterm:[Hidden anchor:hidden[]term]\n\n[#picture]\n.Bird\nimage::bird.svg[]\n\n[bibliography]\n== Sources\n\n* [[[book,Book _title_]]] Book.\n\nSee <<book>>.\n",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let mut expected: Vec<_> = parsed
            .document()
            .references()
            .iter()
            .map(|(id, _)| id)
            .collect();
        expected.sort_unstable();
        for id in [
            "document-header",
            "paragraph",
            "link",
            "span",
            "note",
            "visible",
            "picture",
            "book",
        ] {
            assert!(expected.contains(&id), "missing {id}");
        }
        assert!(!expected.contains(&"hidden"));
        assert!(!expected.contains(&"image"));
        let caption = parsed
            .document()
            .references()
            .get("picture")
            .ok_or("picture")?
            .caption();
        let mut document = Document::from(parsed.document());
        document.rebuild_references_and_toc()?;
        let read = DocumentRef::from(&document);
        let mut actual: Vec<_> = read.references().iter().map(|(id, _)| id).collect();
        actual.sort_unstable();
        assert_eq!(actual, expected);
        let book = read.references().get("book").ok_or("book")?;
        assert!(book.is_bibliography());
        assert!(book.has_automatic_citation());
        assert_eq!(
            book.xreflabel(),
            parsed
                .document()
                .references()
                .get("book")
                .ok_or("original book")?
                .xreflabel()
        );
        assert_eq!(
            read.references().get("picture").ok_or("picture")?.caption(),
            caption
        );
        assert_eq!(serde_json::to_value(read)?, before);
    }
    Ok(())
}

#[test]
fn document_reftext_keeps_its_prepared_snapshot_and_reports_missing_context() -> Result<(), Error> {
    for mode in MODES {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("main.adoc");
        std::fs::write(
            &path,
            "= Guide\n:reftext: *Prepared document label*\n\nxref:main.adoc[]\n",
        )?;
        let parsed = parse_file(
            &path,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let prepared = ContentAttribute::from(
            parsed
                .document()
                .references()
                .get("")
                .ok_or("document top")?
                .xreflabel()
                .ok_or("label")?,
        );
        let mut document = Document::from(parsed.document());
        let mut header = document.take_header().ok_or("header")?;
        header.set_title(title("Changed"));
        document.set_header(Some(header));
        document.rebuild_references_and_toc()?;
        let read = DocumentRef::from(&document);
        assert_eq!(
            read.references().get("").ok_or("document top")?.xreflabel(),
            Some(ContentRef::from(&prepared))
        );
        assert_eq!(
            read.references()
                .get("")
                .ok_or("document top")?
                .title()
                .map(text)
                .as_deref(),
            Some("Changed")
        );
        let mut missing = Document::from(parsed.document());
        let _ = missing.take_references();
        assert!(missing.rebuild_references_and_toc().is_err());
    }
    Ok(())
}

#[test]
fn primary_block_ownership_does_not_become_section_metadata_when_spans_are_equal()
-> Result<(), Error> {
    let parsed = parse(
        "= T\n:sectnums:\n\n[#shared]\n.Same\nParagraph.\n\n[#shared]\n== Same\n",
        &Options::default(),
    )?;
    let mut document = Document::from(parsed.document());
    let mut blocks: Vec<_> = document.take_blocks().collect();
    for block in &mut blocks {
        if let Block::Paragraph(paragraph) = block {
            paragraph.set_location(Location::default());
            paragraph.set_title(title("Same"));
            let mut metadata = paragraph.clone_metadata();
            if let Some(anchor) = &mut metadata.id {
                anchor.set_location(Location::default());
            }
            for anchor in &mut metadata.anchors {
                anchor.set_location(Location::default());
            }
            paragraph.set_metadata(metadata)?;
        } else if let Block::Section(section) = block {
            section.set_location(Location::default());
            section.set_title(title("Same"));
        }
    }
    document.set_blocks(blocks);
    document.rebuild_references_and_toc()?;
    let read = DocumentRef::from(&document);
    let reference = read.references().get("shared").ok_or("shared")?;
    assert_eq!(reference.title().map(text).as_deref(), Some("Same"));
    assert!(reference.section_category().is_none());
    assert!(reference.section_number().is_none());
    let entry = read.toc_entries().next().ok_or("entry")?;
    assert_eq!(reference.location(), entry.location());
    assert!(!reference.matches_toc_entry(entry));
    Ok(())
}

#[test]
fn nested_asciidoc_cells_register_targets_without_entering_the_outer_toc() -> Result<(), Error> {
    for mode in MODES {
        let parsed = parse(
            "= T\n:sectnums:\n\n[#outer]\n== Outer\n\n[cols=\"a\"]\n|===\n|[#cell]\n== Cell\n\nInside.\n|===\n",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let mut document = Document::from(parsed.document());
        document.rebuild_references_and_toc()?;
        let read = DocumentRef::from(&document);
        assert_eq!(
            read.toc_entries().map(TocEntryRef::id).collect::<Vec<_>>(),
            ["outer"]
        );
        assert!(read.references().contains("cell"));
        assert_eq!(
            read.references()
                .get("outer")
                .ok_or("outer")?
                .section_number(),
            Some("1")
        );
        assert_eq!(serde_json::to_value(read)?, before);
    }
    Ok(())
}

#[test]
fn repeated_rebuilds_preserve_owned_equality_and_give_only_copies_new_identity() -> Result<(), Error>
{
    let parsed = parse(
        "= T\n:sectnums:\n\n[#shared]\n== Original\n",
        &Options::default(),
    )?;
    let mut document = Document::from(parsed.document());
    document.rebuild_references_and_toc()?;
    let references: std::collections::HashMap<_, _> = document.take_references().collect();
    document.set_references(references.clone());
    let toc: Vec<_> = document.take_toc_entries().collect();
    document.set_toc_entries(toc.clone());
    let blocks: Vec<_> = document.take_blocks().collect();
    document.set_blocks(blocks.clone());
    document.rebuild_references_and_toc()?;
    assert_eq!(
        std::collections::HashMap::from(DocumentRef::from(&document).references()),
        references
    );
    assert_eq!(
        DocumentRef::from(&document)
            .toc_entries()
            .map(acdc_parser::TocEntry::from)
            .collect::<Vec<_>>(),
        toc
    );
    let rebuilt: Vec<_> = document.take_blocks().collect();
    assert_eq!(rebuilt, blocks);
    let original = rebuilt.first().ok_or("heading")?.clone();
    document.set_blocks([original.clone(), original]);
    document.rebuild_references_and_toc()?;
    let read = DocumentRef::from(&document);
    let entries: Vec<_> = read.toc_entries().collect();
    let [first, copied] = entries.as_slice() else {
        return Err("two headings".into());
    };
    let reference = read.references().get("shared").ok_or("shared")?;
    assert!(reference.matches_toc_entry(*first));
    assert!(!reference.matches_toc_entry(*copied));
    Ok(())
}
