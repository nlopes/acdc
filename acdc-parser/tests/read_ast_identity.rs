use std::{alloc::System, hint::black_box};

use acdc_parser::{
    Anchor, BlockMetadata, CompatibilityMode, Location, Options, Paragraph, SafeMode, Section,
    Title,
    ast::{
        AnchorRef, AttributeValueRef, BlockKind, BlockRef, ElementIdRef, IdOrigin, InlineKind,
        SectionRef, StructuralRef,
    },
    parse, parse_file,
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn headings_select_the_same_primary_id_as_reference_resolution(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "[[header-first]]\n[[header-last]]\n= Document\n\n\
        [[section-first]]\n[[section-last]]\n== Section\n\n\
        [[paragraph-first]]\n[[paragraph-last]]\n.Caption\nParagraph.\n\n\
        [[alias]]\n[id=dedicated]\n== Explicit\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let doc = parsed.document_ref();
    let header = doc.header().ok_or("header")?;
    assert_eq!(doc.id().ok_or("document ID")?.as_str(), "header-last");
    assert_eq!(header.id().ok_or("header ID")?.as_str(), "header-last");
    assert_eq!(
        doc.primary_anchor().ok_or("header anchor")?.id(),
        "header-last"
    );
    assert_eq!(
        doc.anchors().map(AnchorRef::id).collect::<Vec<_>>(),
        ["header-first", "header-last"]
    );
    assert_eq!(
        header.attributes().get("id"),
        Some(AttributeValueRef::Text("header-last"))
    );
    let blocks = doc
        .descendants()
        .filter_map(|node| {
            if let StructuralRef::Block(block) = node {
                Some(block)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let ids = blocks
        .iter()
        .filter_map(|block| block.id().map(ElementIdRef::as_str))
        .collect::<Vec<_>>();
    assert_eq!(ids, ["section-last", "paragraph-first", "dedicated"]);
    for block in &blocks {
        if let Some(id) = block.id() {
            assert_eq!(id.origin(), IdOrigin::Explicit);
            assert!(parsed.document().references.contains_key(id.as_str()));
            assert_eq!(block.primary_anchor().map(AnchorRef::id), Some(id.as_str()));
        }
    }
    let BlockKind::Section(section) = blocks.first().ok_or("first section")?.kind() else {
        return Err("section".into());
    };
    assert_eq!(
        section.attributes().get("id"),
        Some(AttributeValueRef::Text("section-last"))
    );
    assert_eq!(
        section.id().map(ElementIdRef::as_str),
        blocks
            .first()
            .ok_or("first section")?
            .id()
            .map(ElementIdRef::as_str)
    );
    assert_eq!(
        section.anchors().map(AnchorRef::id).collect::<Vec<_>>(),
        ["section-first", "section-last"]
    );
    assert_eq!(
        blocks
            .get(2)
            .ok_or("second section")?
            .anchors()
            .map(AnchorRef::id)
            .collect::<Vec<_>>(),
        ["dedicated", "alias"]
    );
    for unregistered in ["header-first", "section-first", "paragraph-last", "alias"] {
        assert!(!parsed.document().references.contains_key(unregistered));
    }
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[rstest::rstest]
fn generated_section_ids_are_retained_collision_checked_and_not_authored_anchors(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "== Repeated\n\nOne.\n\n== Repeated\n\nTwo.\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    for (block, expected) in parsed
        .document_ref()
        .blocks()
        .zip(["_repeated", "_repeated_2"])
    {
        let BlockKind::Section(section) = block.kind() else {
            return Err("section".into());
        };
        let id = block.id().ok_or("generated ID")?;
        assert_eq!(id.as_str(), expected);
        assert_eq!(id.origin(), IdOrigin::Generated);
        assert_eq!(
            id.location().absolute_start(),
            block.location().absolute_start()
        );
        assert_eq!(section.id().ok_or("typed ID")?.as_str(), expected);
        assert_eq!(
            section.attributes().get("id"),
            Some(AttributeValueRef::Text(expected))
        );
        assert!(block.primary_anchor().is_none());
        assert!(block.anchors().next().is_none());
        assert_eq!(
            parsed
                .document()
                .references
                .get(expected)
                .ok_or("reference")?
                .title
                .as_ref()
                .and_then(Title::content),
            section.title()
        );
    }
    Ok(())
}

#[rstest::rstest]
#[case("Text.")]
#[case("== Heading\n\nText.")]
#[case("====\nBody.\n====")]
#[case("----\nCode.\n----")]
#[case("....\nLiteral.\n....")]
#[case("--\nBody.\n--")]
#[case("****\nBody.\n****")]
#[case("++++\nRaw.\n++++")]
#[case("____\nWords.\n____")]
#[case("[verse]\n____\nWords.\n____")]
#[case("[stem]\n++++\nx\n++++")]
#[case("NOTE: Words.")]
#[case("* Item")]
#[case(". Item")]
#[case("<1> Callout")]
#[case("Term:: Description")]
#[case("image::bird.png[Alt]")]
#[case("audio::sound.ogg[]")]
#[case("video::movie.mp4[]")]
#[case("[discrete]\n== Heading")]
#[case("toc::[]")]
#[case("<<<\n\nAfter.")]
#[case("////\nComment\n////")]
#[case("|===\n|Cell\n|===")]
fn common_block_reads_cover_all_metadata_owners(
    #[case] body: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!(".Caption\n[[alias]]\n[id=primary]\n{body}\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let block = parsed.document_ref().blocks().next().ok_or("block")?;
    assert_eq!(
        block.id().ok_or("primary ID")?.as_str(),
        "primary",
        "{body}"
    );
    assert_eq!(
        block.primary_anchor().ok_or("primary anchor")?.id(),
        "primary"
    );
    assert_eq!(
        block.anchors().map(AnchorRef::id).collect::<Vec<_>>(),
        ["primary", "alias"]
    );
    if matches!(block.kind(), BlockKind::TableOfContents(_)) {
        assert!(block.title().is_none());
    } else {
        assert!(block.title().is_some(), "{body}");
    }
    Ok(())
}

#[test]
fn restricted_and_absent_owners_do_not_synthesize_metadata() -> Result<(), Error> {
    let parsed = parse(
        ".Rule\n[[first]]\n[[second]]\n'''\n\n// Comment\n\n:attribute: value\n",
        &Options::default(),
    )?;
    let mut blocks = parsed.document_ref().blocks();
    let rule = blocks.next().ok_or("rule")?;
    assert_eq!(rule.id().ok_or("rule ID")?.as_str(), "first");
    assert_eq!(
        rule.anchors().map(AnchorRef::id).collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert!(rule.title().is_some());
    for block in blocks {
        assert!(block.id().is_none());
        assert!(block.title().is_none());
        assert!(block.primary_anchor().is_none());
        assert!(block.anchors().next().is_none());
    }
    let parsed = parse("Text.", &Options::default())?;
    assert!(parsed.document_ref().id().is_none());
    assert!(parsed.document_ref().title().is_none());
    assert!(parsed.document_ref().primary_anchor().is_none());
    assert!(parsed.document_ref().anchors().next().is_none());
    Ok(())
}

#[rstest::rstest]
fn inline_identity_is_separate_from_reference_targets_and_footnote_keys(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "Text image:bird.png[Alt,id=image] icon:heart[id=icon] [#span]*bold* anchor:anchor[Label] link:https://example.org[Link,id=link] https://example.org[URL,id=url] mailto:a@example.org[Mail,id=mail] xref:span[Label,id=xref] indexterm2:[Term,id=index] footnote:key[Note.] footnote:key[]",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::Paragraph(paragraph) = parsed
        .document_ref()
        .blocks()
        .next()
        .ok_or("paragraph")?
        .kind()
    else {
        return Err("paragraph".into());
    };
    let ids = paragraph
        .content()
        .inlines()
        .filter_map(|node| node.id().map(|id| (id.as_str(), id.origin())))
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            "image", "icon", "span", "anchor", "link", "url", "mail", "xref", "index"
        ]
        .map(|id| (id, IdOrigin::Explicit))
    );
    for inline in paragraph.content().inlines() {
        if let InlineKind::Image(image) = inline.kind() {
            assert_eq!(
                inline.id().map(ElementIdRef::as_str),
                image.id().map(ElementIdRef::as_str)
            );
        } else if let InlineKind::Bold(span) = inline.kind() {
            assert_eq!(
                inline.id().map(ElementIdRef::as_str),
                span.id().map(ElementIdRef::as_str)
            );
        } else if let InlineKind::Anchor(anchor) = inline.kind() {
            assert_eq!(anchor.element_id().as_str(), anchor.id());
            assert_eq!(
                anchor.element_id().location().absolute_start(),
                anchor.location().absolute_start()
            );
        } else if let InlineKind::Footnote(_) = inline.kind() {
            assert!(inline.id().is_none());
        }
    }
    for target in ["span", "anchor", "link", "url", "mail"] {
        assert!(parsed.document().references.contains_key(target));
    }
    for retained_only in ["image", "icon", "xref", "index", "key"] {
        assert!(!parsed.document().references.contains_key(retained_only));
    }
    Ok(())
}

#[test]
fn caller_built_empty_and_duplicate_ids_remain_explicit_without_generation() -> Result<(), Error> {
    let mut metadata = BlockMetadata::default();
    metadata.id = Some(Anchor::new("", Location::default()));
    metadata.anchors = vec![
        Anchor::new("same", Location::default()),
        Anchor::new("same", Location::default()),
    ];
    let mut paragraph = Paragraph::new(Vec::new(), Location::default());
    paragraph.metadata = metadata;
    paragraph.title = Title::new(Vec::new());
    let owner = acdc_parser::Block::Paragraph(paragraph);
    let read = BlockRef::from(&owner);
    assert_eq!(read.id().ok_or("empty ID")?.as_str(), "");
    assert_eq!(read.id().ok_or("empty ID")?.origin(), IdOrigin::Explicit);
    assert!(read.title().ok_or("empty title")?.is_empty());
    assert_eq!(
        read.anchors().map(AnchorRef::id).collect::<Vec<_>>(),
        ["", "same", "same"]
    );
    let section = Section::new(Title::new(Vec::new()), 1, Vec::new(), Location::default());
    assert!(SectionRef::from(&section).id().is_none());
    Ok(())
}

#[rstest::rstest]
fn expanded_unicode_ids_keep_include_and_table_source_provenance(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let dir = tempfile::tempdir()?;
    let child = ":target: café\n\n[[{target}]]\n== Included\n\n[cols=\"a\"]\n|===\n|[[table-id]]\nText [#span]*Ω*.\n|===\n";
    std::fs::write(dir.path().join("child.adoc"), child)?;
    let path = dir.path().join("root.adoc");
    std::fs::write(&path, "include::child.adoc[]\n")?;
    let parsed = parse_file(
        &path,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let blocks = parsed
        .document_ref()
        .descendants()
        .filter_map(|node| {
            if let StructuralRef::Block(block) = node {
                Some(block)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let section = blocks
        .iter()
        .find(|block| block.id().is_some_and(|id| id.as_str() == "café"))
        .ok_or("section")?;
    let id = section.id().ok_or("expanded ID")?;
    assert_eq!(id.origin(), IdOrigin::Explicit);
    assert_eq!(id.location().start().line(), 3);
    assert!(id.location().start().include_chain().is_some());
    let table_paragraph = blocks
        .iter()
        .find(|block| block.id().is_some_and(|id| id.as_str() == "table-id"))
        .ok_or("table paragraph")?;
    assert_eq!(
        table_paragraph
            .id()
            .ok_or("table ID")?
            .location()
            .start()
            .line(),
        8
    );
    let BlockKind::Paragraph(paragraph) = table_paragraph.kind() else {
        return Err("table paragraph".into());
    };
    let span = paragraph
        .content()
        .inlines()
        .find(|inline| inline.id().is_some())
        .ok_or("span")?;
    let id = span.id().ok_or("span ID")?;
    let start = child.find("span").ok_or("authored ID")?;
    assert_eq!(id.location().absolute_start(), start);
    assert_eq!(id.location().absolute_end(), start + 3);
    assert_eq!(id.location().start().line(), 9);
    assert_eq!(id.location().start().column(), 8);
    Ok(())
}

#[test]
fn identity_and_title_reads_do_not_allocate_or_rebuild_catalogs() -> Result<(), Error> {
    let parsed = parse(
        "[[alias]]\n[id=primary]\n== Heading\n\nText [#span]*bold*.\n\n== Generated\n",
        &Options::default(),
    )?;
    let blocks = parsed.document_ref().blocks().collect::<Vec<_>>();
    let region = Region::new(ALLOCATOR);
    for _ in 0..100 {
        for block in &blocks {
            black_box(block.id());
            black_box(block.title());
            black_box(block.primary_anchor());
            for anchor in block.anchors() {
                black_box(anchor.element_id());
            }
        }
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 0);
    Ok(())
}

#[test]
fn explicit_underscore_ids_are_not_mistaken_for_generated_ids() -> Result<(), Error> {
    let parsed = parse("[id=_same]\n== Same\n\n== Same\n", &Options::default())?;
    let mut blocks = parsed.document_ref().blocks();
    let explicit = blocks
        .next()
        .ok_or("explicit section")?
        .id()
        .ok_or("explicit ID")?;
    let generated = blocks
        .next()
        .ok_or("generated section")?
        .id()
        .ok_or("generated ID")?;
    assert_eq!(
        (explicit.as_str(), explicit.origin()),
        ("_same", IdOrigin::Explicit)
    );
    assert_eq!(
        (generated.as_str(), generated.origin()),
        ("_same_2", IdOrigin::Generated)
    );
    Ok(())
}

#[test]
fn retained_id_origin_survives_direct_metadata_edits() -> Result<(), Error> {
    let parsed = parse(
        "[id=explicit]\n== Heading\n\n== Generated\n",
        &Options::default(),
    )?;
    let mut owners = parsed.document().blocks.iter();
    let acdc_parser::Block::Section(explicit) = owners.next().ok_or("explicit")? else {
        return Err("section".into());
    };
    let mut explicit = explicit.clone();
    explicit.metadata = BlockMetadata::default();
    let id = SectionRef::from(&explicit)
        .id()
        .ok_or("retained explicit ID")?;
    assert_eq!((id.as_str(), id.origin()), ("explicit", IdOrigin::Explicit));
    let acdc_parser::Block::Section(generated) = owners.next().ok_or("generated")? else {
        return Err("section".into());
    };
    let mut generated = generated.clone();
    generated.metadata.id = Some(Anchor::new("override", Location::default()));
    assert_eq!(
        SectionRef::from(&generated)
            .id()
            .ok_or("override")?
            .origin(),
        IdOrigin::Explicit
    );
    generated.metadata.id = None;
    let id = SectionRef::from(&generated)
        .id()
        .ok_or("retained generated ID")?;
    assert_eq!(
        (id.as_str(), id.origin()),
        ("_generated", IdOrigin::Generated)
    );
    assert!(
        SectionRef::from(&generated.with_metadata(BlockMetadata::default()))
            .id()
            .is_none()
    );
    Ok(())
}
