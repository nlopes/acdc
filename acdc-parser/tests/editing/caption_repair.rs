//! Explicit caption repair preserves prepared prefixes and other registrations.

use acdc_parser::{
    Block, CaptionKind, CompatibilityMode, Document, InlineNode, Location, Options, Plain, Title,
    ast::{BlockRef, CaptionRef, DocumentRef, ImageRef, StructuralRef},
    parse,
};

type Error = Box<dyn std::error::Error>;
const MODES: [CompatibilityMode; 2] = [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft];
const SOURCE: &str = include_str!("../../fixtures/tests/caption_repair_snapshots.adoc");

fn caption<'a>(document: DocumentRef<'a>, id: &str) -> Option<CaptionRef<'a>> {
    document.descendants().find_map(|node| {
        let StructuralRef::Block(block) = node else {
            return None;
        };
        if block.id().is_some_and(|value| value.as_str() == id) {
            CaptionRef::resolve(block.attributes()?, document.attributes(), None)
        } else {
            None
        }
    })
}

fn numbered<'a>(document: DocumentRef<'a>, id: &str) -> Option<(&'a str, u32)> {
    if let CaptionRef::Numbered {
        label,
        number: Some(number),
        ..
    } = caption(document, id)?
    {
        Some((label, number.get()))
    } else {
        None
    }
}

#[test]
fn moving_and_removing_blocks_refreshes_numbers_and_aliases_without_replaying_attributes()
-> Result<(), Error> {
    for mode in MODES {
        let parsed = parse(
            SOURCE,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let original = serde_json::to_value(parsed.document())?;
        let mut document = Document::from(parsed.document());
        let mut references: Vec<_> = document.take_references().collect();
        let two = references
            .iter()
            .find(|(id, _)| *id == "two")
            .ok_or("two")?
            .1
            .clone();
        references.push(("alias", two));
        document.set_references(references);
        let mut blocks: Vec<_> = document.take_blocks().collect();
        let second = blocks
            .iter()
            .position(|block| {
                BlockRef::from(block)
                    .id()
                    .is_some_and(|anchor| anchor.as_str() == "two")
            })
            .ok_or("second")?;
        let second = blocks.remove(second);
        blocks.retain(|block| {
            BlockRef::from(block)
                .id()
                .is_none_or(|anchor| anchor.as_str() != "one")
        });
        blocks.insert(0, second);
        document.set_blocks(blocks);
        document.set_attributes(
            Options::with_attributes([("figure-caption", "Wrong")])?.into_document_attributes(),
        );
        document.rebuild_captions()?;
        let read = DocumentRef::from(&document);
        assert_eq!(numbered(read, "two"), Some(("Later", 1)));
        assert_eq!(numbered(read, "blank"), Some(("", 2)));
        assert_eq!(
            read.references().get("two").ok_or("two")?.caption(),
            caption(read, "two")
        );
        assert_eq!(
            read.references().get("alias").ok_or("alias")?.caption(),
            caption(read, "two")
        );
        assert!(
            read.references()
                .get("one")
                .ok_or("stale membership retained")?
                .caption()
                .is_none()
        );
        assert_eq!(caption(read, "unnumbered"), Some(CaptionRef::Unnumbered));
        assert_eq!(
            caption(read, "custom"),
            caption(parsed.document(), "custom")
        );
        assert_eq!(caption(read, "empty"), caption(parsed.document(), "empty"));
        assert_eq!(serde_json::to_value(parsed.document())?, original);
    }
    Ok(())
}

#[test]
fn unchanged_repair_preserves_source_snapshots_and_all_counter_categories() -> Result<(), Error> {
    for mode in MODES {
        let parsed = parse(
            SOURCE,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let mut document = Document::from(parsed.document());
        document.rebuild_captions()?;
        let read = DocumentRef::from(&document);
        assert_eq!(serde_json::to_value(read)?, before);
        for (id, label, number) in [
            ("one", "First", 1),
            ("two", "Later", 2),
            ("blank", "", 3),
            ("cell", "", 4),
            ("inner", "Example", 1),
            ("outer", "Example", 2),
            ("source", "Code", 1),
            ("table", "Table", 1),
        ] {
            assert_eq!(numbered(read, id), Some((label, number)), "{id}");
            assert_eq!(
                read.references().get(id).ok_or("target")?.caption(),
                caption(read, id),
                "{id}"
            );
        }
        let blocks: Vec<_> = document.take_blocks().collect();
        document.set_blocks(blocks.clone());
        let references: std::collections::HashMap<_, _> = document.take_references().collect();
        document.set_references(references.clone());
        document.rebuild_captions()?;
        assert_eq!(document.take_blocks().collect::<Vec<_>>(), blocks);
        assert_eq!(
            document
                .take_references()
                .collect::<std::collections::HashMap<_, _>>(),
            references
        );
    }
    Ok(())
}

#[test]
fn inline_targets_keep_caption_ownership_ahead_of_duplicate_blocks() -> Result<(), Error> {
    for mode in MODES {
        let parsed = parse(
            "= T\n\nanchor:shared[]First target.\n\n[#shared]\n.Image\nimage::bird.svg[]\n",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let mut document = Document::from(parsed.document());
        document.rebuild_captions()?;
        let read = DocumentRef::from(&document);
        assert_eq!(numbered(read, "shared"), Some(("Figure", 1)));
        assert!(
            read.references()
                .get("shared")
                .ok_or("first")?
                .caption()
                .is_none()
        );
    }
    Ok(())
}

#[test]
fn missing_prepared_context_fails_before_numbering_or_changing_reference_snapshots()
-> Result<(), Error> {
    let parsed = parse(SOURCE, &Options::default())?;
    let mut document = Document::from(parsed.document());
    let mut blocks: Vec<_> = document.take_blocks().collect();
    for block in &mut blocks {
        if let Block::Image(image) = block {
            let mut metadata = image.clone_metadata();
            if ImageRef::from(&*image)
                .id()
                .is_some_and(|anchor| anchor.as_str() == "two")
            {
                metadata.caption = None;
                image.set_metadata(metadata)?;
            }
        }
    }
    blocks.reverse();
    document.set_blocks(blocks.clone());
    let references: std::collections::HashMap<_, _> = document.take_references().collect();
    document.set_references(references.clone());
    assert!(document.rebuild_captions().is_err());
    assert_eq!(document.take_blocks().collect::<Vec<_>>(), blocks);
    assert_eq!(
        document
            .take_references()
            .collect::<std::collections::HashMap<_, _>>(),
        references
    );
    Ok(())
}

#[test]
fn adding_or_removing_a_title_updates_only_the_prepared_automatic_counter() -> Result<(), Error> {
    let parsed = parse(
        "= T\n\n[#first]\nimage::first.svg[]\n\n[#second]\n.Second\nimage::second.svg[]\n",
        &Options::default(),
    )?;
    let mut document = Document::from(parsed.document());
    let mut blocks: Vec<_> = document.take_blocks().collect();
    for block in &mut blocks {
        if let Block::Image(image) = block {
            if ImageRef::from(&*image)
                .id()
                .is_some_and(|anchor| anchor.as_str() == "first")
            {
                image.set_title(Title::new(vec![InlineNode::PlainText(Plain::new(
                    "Added",
                    Location::default(),
                ))]));
            } else {
                image.set_title(Title::default());
            }
        }
    }
    document.set_blocks(blocks);
    document.rebuild_captions()?;
    let read = DocumentRef::from(&document);
    assert_eq!(numbered(read, "first"), Some(("Figure", 1)));
    assert!(matches!(
        caption(read, "second"),
        Some(CaptionRef::Numbered { number: None, .. })
    ));
    assert_eq!(document.highest_caption_number(CaptionKind::Figure), 1);
    assert_eq!(
        read.references().get("first").ok_or("first")?.caption(),
        caption(read, "first")
    );
    assert_eq!(
        read.references().get("second").ok_or("second")?.caption(),
        caption(read, "second")
    );
    Ok(())
}

#[test]
fn repeated_selected_includes_keep_prefix_snapshots_and_leave_footnotes_unchanged()
-> Result<(), Error> {
    for mode in MODES {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("main.adoc");
        std::fs::write(
            directory.path().join("part.adoc"),
            "Discarded.\n\n// tag::selected[]\n[#shared]\n.Image footnote:[Kept snapshot]\nimage::bird.svg[]\n// end::selected[]\n",
        )?;
        std::fs::write(
            &path,
            "= T\n:figure-caption: First\n\ninclude::part.adoc[tag=selected]\n\n:figure-caption: Second\n\ninclude::part.adoc[tag=selected]\n",
        )?;
        let parsed = acdc_parser::parse_file(
            &path,
            &Options::builder()
                .with_safe_mode(acdc_parser::SafeMode::Unsafe)
                .with_compatibility_mode(mode)
                .build()?,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let mut document = Document::from(parsed.document());
        let mut removed = false;
        let blocks = document.take_blocks();
        document.set_blocks(blocks.filter(|block| {
            if !removed && matches!(block, Block::Image(_)) {
                removed = true;
                false
            } else {
                true
            }
        }));
        document.rebuild_captions()?;
        let read = DocumentRef::from(&document);
        assert_eq!(numbered(read, "shared"), Some(("Second", 1)));
        assert_eq!(
            read.references().get("shared").ok_or("shared")?.caption(),
            caption(read, "shared")
        );
        assert_eq!(read.footnotes().count(), 2);
        assert_eq!(serde_json::to_value(parsed.document())?, before);
        document.rebuild_references_and_toc()?;
        let read = DocumentRef::from(&document);
        assert_eq!(
            read.references().get("shared").ok_or("shared")?.caption(),
            caption(read, "shared")
        );
        assert_eq!(read.footnotes().count(), 2);
    }
    Ok(())
}

#[test]
fn header_and_heading_targets_keep_ownership_ahead_of_captioned_duplicates() -> Result<(), Error> {
    for first in ["[#shared]\n= Header\n", "= T\n\n[#shared]\n== Heading\n"] {
        let source = format!("{first}\n[#shared]\n.Image\nimage::bird.svg[]\n");
        let parsed = parse(&source, &Options::default())?;
        let mut document = Document::from(parsed.document());
        document.rebuild_captions()?;
        let read = DocumentRef::from(&document);
        assert_eq!(numbered(read, "shared"), Some(("Figure", 1)));
        assert!(
            read.references()
                .get("shared")
                .ok_or("shared")?
                .caption()
                .is_none()
        );
    }
    Ok(())
}

#[test]
fn an_incompatible_numbered_category_requires_new_prepared_context() -> Result<(), Error> {
    let parsed = parse(".Image\nimage::bird.svg[]\n", &Options::default())?;
    let mut document = Document::from(parsed.document());
    let mut blocks: Vec<_> = document.take_blocks().collect();
    let Block::Image(image) = blocks.first_mut().ok_or("image")? else {
        return Err("image".into());
    };
    let mut metadata = image.clone_metadata();
    metadata.caption = Some(acdc_parser::Caption::Numbered {
        kind: CaptionKind::Listing,
        label: "Wrong category".into(),
        number: None,
    });
    image.set_metadata(metadata)?;
    document.set_blocks(blocks.clone());
    assert!(document.rebuild_captions().is_err());
    assert_eq!(document.take_blocks().collect::<Vec<_>>(), blocks);
    Ok(())
}
