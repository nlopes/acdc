use std::{alloc::System, hint::black_box, num::NonZeroU32, ptr};

use acdc_parser::{
    Block, Caption, CaptionKind, CompatibilityMode, ContentAttribute, Document, Options,
    ReferenceTarget,
    ast::{CaptionRef, DocumentRef, ReferenceTargetRef},
    parse,
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

type Error = Box<dyn std::error::Error>;

const SOURCE: &str = "= Doc\n:listing-caption: Listing\n\n\
    [#figure]\n.Figure title\nimage::figure.svg[]\n\n\
    [#table]\n.Table title\n|===\n|Cell\n|===\n\n\
    [#example]\n.Example title\n====\nBody.\n====\n\n\
    [#listing]\n.Listing title\n----\ncode\n----\n\n\
    [#plain]\nPlain paragraph.\n\n\
    :caption: Global:\n\n[#global]\n.Global title\nimage::global.svg[]\n\n\
    :caption!:\n\n[#rich]\n.Rich title\n[caption='*Préfixe*: ']\nimage::rich.svg[]\n\n\
    [#empty]\n.Empty title\n[caption='']\nimage::empty.svg[]\n\n\
    :figure-caption!:\n\n[#disabled]\n.Disabled title\nimage::disabled.svg[]\n";

#[rstest::rstest]
fn highest_caption_numbers_use_stored_tree_values_without_allocating(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    for kind in [
        CaptionKind::Example,
        CaptionKind::Table,
        CaptionKind::Figure,
        CaptionKind::Listing,
    ] {
        assert_eq!(parsed.document().highest_caption_number(kind), 1);
        assert_eq!(
            DocumentRef::from(&Document::default()).highest_caption_number(kind),
            0
        );
    }
    let mut document = Document::default();
    document.set_blocks(parsed.document().blocks().map(Block::from));
    document.set_references(
        parsed
            .document()
            .references()
            .iter()
            .map(|(id, reference)| (id, acdc_parser::ReferenceTarget::from(reference))),
    );
    let mut references: std::collections::HashMap<_, _> = document.take_references().collect();
    references
        .get_mut("figure")
        .ok_or("reference")?
        .set_caption(Some(Caption::Numbered {
            kind: CaptionKind::Figure,
            label: "Catalog".into(),
            number: NonZeroU32::new(99),
        }));

    document.set_references(references);

    for (caption, expected_figure, expected_example) in [
        (None, 0, 1),
        (Some(Caption::Unnumbered), 0, 1),
        (Some(Caption::Custom("Custom".into())), 0, 1),
        (
            Some(Caption::Numbered {
                kind: CaptionKind::Figure,
                label: "".into(),
                number: None,
            }),
            0,
            1,
        ),
        (
            Some(Caption::Numbered {
                kind: CaptionKind::Figure,
                label: "".into(),
                number: NonZeroU32::new(41),
            }),
            41,
            1,
        ),
        (
            Some(Caption::Numbered {
                kind: CaptionKind::Example,
                label: "Other category".into(),
                number: NonZeroU32::new(17),
            }),
            0,
            17,
        ),
    ] {
        let mut blocks: Vec<_> = document.take_blocks().collect();
        let image = blocks
            .iter_mut()
            .find_map(|block| {
                if let Block::Image(image) = block {
                    Some(image)
                } else {
                    None
                }
            })
            .ok_or("image")?;
        image.set_title(acdc_parser::Title::default());
        let mut metadata = image.take_metadata();
        metadata.caption = caption;
        image.set_metadata(metadata)?;
        document.set_blocks(blocks);
        let before = serde_json::to_value(&document)?;
        let region = Region::new(ALLOCATOR);
        let handle = DocumentRef::from(&document);
        let numbers = black_box([
            handle.highest_caption_number(CaptionKind::Figure),
            handle.highest_caption_number(CaptionKind::Example),
            handle.highest_caption_number(CaptionKind::Table),
            handle.highest_caption_number(CaptionKind::Listing),
        ]);
        let stats = region.change();
        assert_eq!(numbers, [expected_figure, expected_example, 1, 1]);
        assert_eq!(stats.allocations, 0);
        assert_eq!(stats.reallocations, 0);
        assert_eq!(serde_json::to_value(&document)?, before);
    }
    Ok(())
}

#[test]
fn constructed_caption_states_do_not_gain_content_or_ordinals() -> Result<(), Error> {
    let parsed = parse(SOURCE, &Options::default())?;
    let mut reference = ReferenceTarget::from(
        parsed
            .document()
            .references()
            .get("figure")
            .ok_or("figure")?,
    );
    reference.set_caption(None);
    assert!(ReferenceTargetRef::from(&reference).caption().is_none());
    reference.set_caption(Some(Caption::Custom("".into())));
    assert_eq!(
        ReferenceTargetRef::from(&reference).caption(),
        Some(CaptionRef::Custom(""))
    );
    reference.set_caption(Some(Caption::CustomContent(ContentAttribute::default())));
    assert!(
        matches!(ReferenceTargetRef::from(&reference).caption(), Some(CaptionRef::CustomContent(content)) if content.is_empty())
    );
    reference.set_caption(Some(Caption::Unnumbered));
    assert_eq!(
        ReferenceTargetRef::from(&reference).caption(),
        Some(CaptionRef::Unnumbered)
    );
    for number in [None, NonZeroU32::new(7)] {
        let caption = Caption::Numbered {
            kind: CaptionKind::Figure,
            label: "".into(),
            number,
        };
        assert_eq!(
            CaptionRef::from(&caption),
            CaptionRef::Numbered {
                kind: CaptionKind::Figure,
                label: "",
                number
            }
        );
    }
    let custom = Caption::Custom(String::from("Owned prefix. ").into());
    let CaptionRef::Custom(label) = CaptionRef::from(&custom) else {
        return Err("scalar custom caption".into());
    };
    let Caption::Custom(original) = &custom else {
        return Err("owner custom caption".into());
    };
    assert!(ptr::eq(label, original.as_ref()));
    Ok(())
}

#[test]
fn caption_reads_preserve_registered_snapshots_after_owner_edits() -> Result<(), Error> {
    let parsed = parse(SOURCE, &Options::default())?;
    let mut document = Document::default();
    document.set_blocks(parsed.document().blocks().map(Block::from));
    document.set_references(
        parsed
            .document()
            .references()
            .iter()
            .map(|(id, reference)| (id, acdc_parser::ReferenceTarget::from(reference))),
    );
    let mut references: std::collections::HashMap<_, _> = document.take_references().collect();
    references.insert(
        "alias",
        references.get("figure").ok_or("alias target")?.clone(),
    );
    document.set_references(references);
    assert_eq!(
        DocumentRef::from(&document)
            .references()
            .get("alias")
            .ok_or("alias")?
            .caption(),
        DocumentRef::from(&document)
            .references()
            .get("figure")
            .ok_or("figure")?
            .caption()
    );
    let mut blocks: Vec<_> = document.take_blocks().collect();
    let image = blocks
        .iter_mut()
        .find_map(|block| {
            if let Block::Image(image) = block {
                Some(image)
            } else {
                None
            }
        })
        .ok_or("image")?;
    let mut metadata = image.take_metadata();
    metadata.caption = Some(Caption::Custom("Changed tree".into()));
    image.set_metadata(metadata)?;
    document.set_blocks(blocks);
    assert!(
        matches!(DocumentRef::from(&document).references().get("figure").ok_or("figure")?.caption(), Some(CaptionRef::Numbered { number: Some(number), .. }) if number.get() == 1)
    );
    let mut references: std::collections::HashMap<_, _> = document.take_references().collect();
    references
        .get_mut("figure")
        .ok_or("registered figure")?
        .set_caption(Some(Caption::Custom("Changed catalog".into())));
    document.set_references(references);
    assert_eq!(
        DocumentRef::from(&document)
            .references()
            .get("figure")
            .ok_or("figure")?
            .caption(),
        Some(CaptionRef::Custom("Changed catalog"))
    );
    Ok(())
}

#[test]
fn caption_reads_are_shared_and_allocate_nothing() -> Result<(), Error> {
    fn shared<T: Copy + Send + Sync>() {}
    shared::<CaptionRef<'_>>();
    let parsed = parse(SOURCE, &Options::default())?;
    let references = parsed.document().references();
    let region = Region::new(ALLOCATOR);
    for (_, reference) in references.iter() {
        if let Some(caption) = reference.caption() {
            black_box(caption);
            if let CaptionRef::CustomContent(content) = caption {
                black_box(content.inlines().count());
            }
        }
    }
    for block in parsed.document().blocks() {
        black_box(CaptionKind::for_block(block));
        if let Some(attributes) = block.attributes() {
            black_box(CaptionRef::resolve(
                attributes,
                parsed.document().attributes(),
                Some(CaptionKind::Figure),
            ));
        }
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 0);
    Ok(())
}
