use std::{alloc::System, hint::black_box, num::NonZeroU32, ptr};

use acdc_parser::{
    Block, Caption, CaptionKind, CompatibilityMode, ContentAttribute, Document, Options, SafeMode,
    ast::{CaptionRef, ContentRef, DocumentRef, InlineKind, InlineRef, ReferenceRef},
    parse, parse_file,
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
fn registered_captions_preserve_values_and_json(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let owner = parsed.document();
    let before = serde_json::to_value(owner)?;
    let catalog = parsed.document_ref().references();
    for (id, category) in [
        ("figure", CaptionKind::Figure),
        ("table", CaptionKind::Table),
        ("example", CaptionKind::Example),
        ("listing", CaptionKind::Listing),
    ] {
        let Some(CaptionRef::Numbered {
            kind,
            label,
            number,
        }) = catalog.get(id).ok_or("numbered target")?.caption()
        else {
            return Err(format!("missing numbered caption: {id}").into());
        };
        assert_eq!(kind, category);
        assert_eq!(number.map(NonZeroU32::get), Some(1));
        let Some(Caption::Numbered {
            label: original, ..
        }) = &owner.references.get(id).ok_or("owner")?.caption
        else {
            return Err("owner numbered caption".into());
        };
        assert!(ptr::eq(label, original.as_ref()));
    }
    assert!(catalog.get("plain").ok_or("plain")?.caption().is_none());
    assert_eq!(
        catalog.get("global").ok_or("global")?.caption(),
        Some(CaptionRef::Custom("Global:"))
    );
    assert_eq!(
        catalog.get("disabled").ok_or("disabled")?.caption(),
        Some(CaptionRef::Unnumbered)
    );
    let Some(CaptionRef::CustomContent(empty)) = catalog.get("empty").ok_or("empty")?.caption()
    else {
        return Err("present empty caption".into());
    };
    assert!(empty.is_empty());
    let Some(CaptionRef::CustomContent(rich)) = catalog.get("rich").ok_or("rich")?.caption() else {
        return Err("formatted caption".into());
    };
    let Some(InlineKind::Bold(prefix)) = rich.inlines().next().map(InlineRef::kind) else {
        return Err("caption formatting".into());
    };
    assert_eq!(prefix.content().inlines().count(), 1);
    let Some(Caption::CustomContent(original)) =
        &owner.references.get("rich").ok_or("rich owner")?.caption
    else {
        return Err("owner content".into());
    };
    assert_eq!(rich, ContentRef::from(original));
    assert_eq!(serde_json::to_value(owner)?, before);
    Ok(())
}

#[test]
fn constructed_caption_states_do_not_gain_content_or_ordinals() -> Result<(), Error> {
    let parsed = parse(SOURCE, &Options::default())?;
    let mut reference = parsed
        .document()
        .references
        .get("figure")
        .ok_or("figure")?
        .clone();
    reference.caption = None;
    assert!(ReferenceRef::from(&reference).caption().is_none());
    reference.caption = Some(Caption::Custom("".into()));
    assert_eq!(
        ReferenceRef::from(&reference).caption(),
        Some(CaptionRef::Custom(""))
    );
    reference.caption = Some(Caption::CustomContent(ContentAttribute::default()));
    assert!(
        matches!(ReferenceRef::from(&reference).caption(), Some(CaptionRef::CustomContent(content)) if content.is_empty())
    );
    reference.caption = Some(Caption::Unnumbered);
    assert_eq!(
        ReferenceRef::from(&reference).caption(),
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
    document.blocks = parsed.document().blocks.clone();
    document.references = parsed.document().references.clone();
    document.references.insert(
        "alias",
        document
            .references
            .get("figure")
            .ok_or("alias target")?
            .clone(),
    );
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
    let image = document
        .blocks
        .iter_mut()
        .find_map(|block| {
            if let Block::Image(image) = block {
                Some(image)
            } else {
                None
            }
        })
        .ok_or("image")?;
    image.metadata.caption = Some(Caption::Custom("Changed tree".into()));
    assert!(
        matches!(DocumentRef::from(&document).references().get("figure").ok_or("figure")?.caption(), Some(CaptionRef::Numbered { number: Some(number), .. }) if number.get() == 1)
    );
    document
        .references
        .get_mut("figure")
        .ok_or("registered figure")?
        .caption = Some(Caption::Custom("Changed catalog".into()));
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

#[rstest::rstest]
fn registered_caption_content_preserves_include_locations(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(false, true)] nested_cell: bool,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let included = directory.path().join("part.adoc");
    let source = "// tag::selected[]\n[#included,caption='*λ* footnote:[Included note]: ']\n.Title\n====\nBody.\n====\n// end::selected[]\n";
    std::fs::write(&included, source)?;
    let root = directory.path().join("root.adoc");
    let main = if nested_cell {
        "[cols=a]\n|===\n|\ninclude::part.adoc[tag=selected]\n|===\n"
    } else {
        "include::part.adoc[tag=selected]\n"
    };
    std::fs::write(&root, main)?;
    let parsed = parse_file(
        &root,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let Some(CaptionRef::CustomContent(content)) = parsed
        .document_ref()
        .references()
        .get("included")
        .ok_or("included target")?
        .caption()
    else {
        return Err("included caption".into());
    };
    assert_eq!(parsed.document_ref().footnotes().count(), 1);
    let node = content.inlines().next().ok_or("caption prefix")?;
    assert!(matches!(node.kind(), InlineKind::Bold(_)));
    let original = parsed
        .document()
        .references
        .get("included")
        .and_then(|reference| reference.caption.as_ref())
        .ok_or("owner caption")?;
    let Caption::CustomContent(original) = original else {
        return Err("owner custom caption".into());
    };
    let original = original.first().ok_or("owner prefix")?;
    assert_eq!(
        acdc_parser::Location::from(node.location()),
        *original.location()
    );
    let resolved = parsed.resolve_location(node.location());
    assert_eq!(
        resolved.file.ok_or("source file")?.canonicalize()?,
        included.canonicalize()?
    );
    assert_eq!(resolved.location.start.line, 2);
    assert_eq!(
        source.get(resolved.location.absolute_start..=resolved.location.absolute_end),
        Some("*λ*")
    );
    Ok(())
}

#[test]
fn caption_reads_are_shared_and_allocate_nothing() -> Result<(), Error> {
    fn shared<T: Copy + Send + Sync>() {}
    shared::<CaptionRef<'_>>();
    let parsed = parse(SOURCE, &Options::default())?;
    let references = parsed.document_ref().references();
    let region = Region::new(ALLOCATOR);
    for (_, reference) in references.iter() {
        if let Some(caption) = reference.caption() {
            black_box(caption);
            if let CaptionRef::CustomContent(content) = caption {
                black_box(content.inlines().count());
            }
        }
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 0);
    Ok(())
}
