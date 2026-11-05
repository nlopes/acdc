//! Detached catalog edits retain presence, derived snapshots and source lifetimes.

use std::{alloc::System, fmt::Write, hint::black_box};

use acdc_parser::{
    Block, Caption, Comment, CommentKind, CompatibilityMode, ContentAttribute, Location, Options,
    Reference, SectionKind, Title, TocEntry,
    ast::{BlockKind, BlockRef, ReferenceRef, TocEntryRef},
    parse,
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

type Error = Box<dyn std::error::Error>;

const SOURCE: &str = "= Book\n:sectnums:\n\n[[first,First label]]\n== *First*\n\nBody.\n\n[#picture,reftext='Picture *label*']\n.Caption\nimage::bird.svg[Bird]\n\n[bibliography]\n== Sources\n\n* [[[book]]] A book.\n\nSee <<book>>.\n";

#[rstest::rstest]
fn selected_copies_and_part_round_trips_preserve_all_catalog_snapshots(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let original_document = serde_json::to_value(parsed.document())?;
    let references = parsed.document().references();
    let bibliography = references.get("book").ok_or("book")?;
    assert!(bibliography.is_bibliography());
    assert!(bibliography.has_automatic_citation());
    assert_eq!(
        references.get("first").ok_or("first")?.section_number(),
        Some("1")
    );
    assert!(
        references
            .get("picture")
            .ok_or("picture")?
            .caption()
            .is_some()
    );
    for (_, reference) in references.iter() {
        let mut copied = Reference::from(reference);
        let copied_read = ReferenceRef::from(&copied);
        assert_eq!(copied_read.title(), reference.title());
        assert_eq!(copied_read.xreflabel(), reference.xreflabel());
        assert_eq!(copied_read.section_name(), reference.section_name());
        assert_eq!(copied_read.section_number(), reference.section_number());
        assert_eq!(copied_read.is_bibliography(), reference.is_bibliography());
        assert_eq!(
            copied_read.has_automatic_citation(),
            reference.has_automatic_citation()
        );
        let original = copied.clone();
        let title = copied.take_title();
        let label = copied.take_xreflabel();
        let caption = copied.take_caption();
        assert!(ReferenceRef::from(&copied).title().is_none());
        assert!(ReferenceRef::from(&copied).xreflabel().is_none());
        assert!(ReferenceRef::from(&copied).caption().is_none());
        copied.set_title(title);
        copied.set_xreflabel(label);
        copied.set_caption(caption);
        copied.set_location(Location::from(reference.location()));
        assert_eq!(copied, original);
    }
    for entry in parsed.document().toc_entries() {
        let mut copied = TocEntry::from(entry);
        let original = copied.clone();
        let title = copied.take_title();
        let label = copied.take_xreflabel();
        assert!(TocEntryRef::from(&copied).title().is_none());
        assert!(TocEntryRef::from(&copied).xreflabel().is_none());
        copied.set_title(title);
        copied.set_xreflabel(label);
        assert_eq!(copied, original);
        assert_eq!(TocEntryRef::from(&copied).number(), entry.number());
    }
    assert_eq!(serde_json::to_value(parsed.document())?, original_document);
    Ok(())
}

#[test]
fn reference_presence_keeps_empty_titles_labels_and_captions() {
    let mut reference = Reference::new(Location::default());
    for (title, present) in [
        (None, None),
        (Some(Title::default()), Some(false)),
        (Some(Title::new(Vec::new())), Some(true)),
    ] {
        reference.set_title(title.clone());
        assert_eq!(
            ReferenceRef::from(&reference).title_is_present(),
            present == Some(true)
        );
        assert_eq!(
            ReferenceRef::from(&reference).title().is_some(),
            present.is_some()
        );
        assert_eq!(
            reference.take_title().as_ref().map(Title::is_present),
            present
        );
        reference.set_title(title);
        if let Some(content) = ReferenceRef::from(&reference).title() {
            assert!(content.is_empty());
        }
    }
    for label in [None, Some(ContentAttribute::new(Vec::new()))] {
        reference.set_xreflabel(label.clone());
        assert_eq!(
            ReferenceRef::from(&reference).xreflabel().is_some(),
            label.is_some()
        );
        assert_eq!(reference.take_xreflabel(), label);
        assert!(ReferenceRef::from(&reference).xreflabel().is_none());
    }
    for caption in [
        None,
        Some(Caption::CustomContent(ContentAttribute::new(Vec::new()))),
        Some(Caption::Unnumbered),
    ] {
        reference.set_caption(caption.clone());
        assert_eq!(
            ReferenceRef::from(&reference).caption().is_some(),
            caption.is_some()
        );
        assert_eq!(reference.take_caption(), caption);
        assert!(ReferenceRef::from(&reference).caption().is_none());
    }
}

#[rstest::rstest]
fn reference_content_transfers_outlive_the_previous_record(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let source = parsed
        .document()
        .references()
        .get("picture")
        .ok_or("picture")?;
    let original = Reference::from(source);
    let (title, label, caption) = {
        let mut local = original.clone();
        (
            local.take_title(),
            local.take_xreflabel(),
            local.take_caption(),
        )
    };
    let mut rebuilt = Reference::new(Location::from(source.location()));
    rebuilt.set_title(title);
    rebuilt.set_xreflabel(label);
    rebuilt.set_caption(caption);
    assert_eq!(rebuilt, original);
    Ok(())
}

#[test]
fn constructed_toc_entry_preserves_empty_content_and_source_lifetime() -> Result<(), Error> {
    let id = String::from("original");
    let previous = {
        let mut entry = TocEntry::new(&id, Title::new(Vec::new()), 1, Location::default());
        entry.set_xreflabel(Some(ContentAttribute::new(Vec::new())));
        entry.set_level(4);
        entry.set_kind(SectionKind::Appendix);
        entry.set_location(Location::default());
        let read = TocEntryRef::from(&entry);
        assert_eq!(read.level(), 4);
        assert_eq!(read.kind(), SectionKind::Appendix);
        assert!(read.number().is_none());
        assert!(read.title().ok_or("title")?.is_empty());
        assert!(read.xreflabel().ok_or("label")?.is_empty());
        assert!(entry.take_title().is_present());
        assert!(TocEntryRef::from(&entry).title().is_none());
        entry.replace_id("replacement")
    };
    assert_eq!(previous, "original");
    assert_eq!(previous.as_ptr(), id.as_ptr());
    Ok(())
}

#[rstest::rstest]
fn toc_content_transfers_and_scalar_edits_do_not_rename_or_renumber_the_document(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let original_document = serde_json::to_value(parsed.document())?;
    let entry = parsed
        .document()
        .toc_entries()
        .find(|entry| entry.id() == "first")
        .ok_or("entry")?;
    let original = TocEntry::from(entry);
    let (title, label) = {
        let mut local = original.clone();
        (local.take_title(), local.take_xreflabel())
    };
    let mut rebuilt = original.clone();
    rebuilt.set_title(title);
    rebuilt.set_xreflabel(label);
    assert_eq!(rebuilt, original);
    rebuilt.set_level(4);
    rebuilt.set_kind(SectionKind::Appendix);
    rebuilt.set_location(Location::default());
    assert_eq!(rebuilt.replace_id("replacement"), "first");
    assert_eq!(TocEntryRef::from(&rebuilt).number(), entry.number());
    assert_eq!(TocEntryRef::from(&rebuilt).id(), "replacement");
    assert!(parsed.document().references().contains("first"));
    assert!(!parsed.document().references().contains("replacement"));
    assert_eq!(serde_json::to_value(parsed.document())?, original_document);
    Ok(())
}

fn cost<T>(copy: impl FnOnce() -> T) -> (usize, usize) {
    let region = Region::new(ALLOCATOR);
    let value = black_box(copy());
    let stats = region.change();
    drop(value);
    (stats.allocations, stats.bytes_allocated)
}

#[test]
fn selected_catalog_copy_cost_does_not_grow_with_unrelated_headings() -> Result<(), Error> {
    let mut references = Vec::new();
    let mut entries = Vec::new();
    for others in [0, 1000] {
        let mut source = String::from("= Doc\n:sectnums:\n\n[#first]\n== First\n\nBody.\n\n");
        for index in 0..others {
            write!(source, "== Extra {index}\n\nBody.\n\n")?;
        }
        let parsed = parse(&source, &Options::default())?;
        assert_eq!(parsed.document().toc_entries().count(), 1 + others);
        let reference = parsed
            .document()
            .references()
            .get("first")
            .ok_or("reference")?;
        references.push(cost(|| Reference::from(reference)));
        let entry = parsed.document().toc_entries().next().ok_or("entry")?;
        entries.push(cost(|| TocEntry::from(entry)));
    }
    assert_eq!(references.first(), references.last());
    assert_eq!(entries.first(), entries.last());
    Ok(())
}

#[test]
fn common_metadata_copy_cost_does_not_grow_with_block_content() -> Result<(), Error> {
    let mut costs = Vec::new();
    for words in [1, 1000] {
        let source = format!(
            "[role=role,options=opt,extension=value]\n.Title\n{}\n",
            "word *bold* ".repeat(words)
        );
        let parsed = parse(&source, &Options::default())?;
        let owner = Block::from(parsed.document().blocks().next().ok_or("block")?);
        let BlockKind::Paragraph(paragraph) = BlockRef::from(&owner).kind() else {
            return Err("paragraph".into());
        };
        assert!(paragraph.content().inlines().count() >= words);
        assert!(owner.clone_metadata().is_some());
        costs.push(cost(|| owner.clone_metadata()));
    }
    assert_eq!(costs.first(), costs.last());
    let comment = Block::Comment(Comment::new(
        CommentKind::Line,
        "literal",
        Location::default(),
    ));
    assert!(comment.clone_metadata().is_none());
    Ok(())
}
