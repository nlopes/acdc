//! Selected header copies do not materialize unrelated document content.

use std::{alloc::System, hint::black_box, ptr};

use acdc_parser::{
    Author, CompatibilityMode, Document, Header, Options, Title,
    ast::{AuthorRef, HeaderRef},
    parse,
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

type Error = Box<dyn std::error::Error>;

const HEADER: &str = "[#heading.hero%featured,extension=kept]\n= *Heading*: Subtitle\nÉlodie Marie Durand <elodie@example.org>\n\n";

#[rstest::rstest]
fn selected_copies_keep_values_and_edits_leave_the_parse_unchanged(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("{HEADER}Body.\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let mut copy = {
        let header = parsed.document().header().ok_or("header")?;
        Header::from(header)
    };
    let expected = Document::from(parsed.document())
        .take_header()
        .ok_or("header")?;
    assert_eq!(copy, expected);
    assert_eq!(
        serde_json::to_value(&copy)?,
        serde_json::to_value(&expected)?
    );
    copy.set_title(Title::new(Vec::new()));
    let mut metadata = copy.clone().take_metadata();
    metadata.roles.push("selected");
    copy.set_metadata(metadata)?;
    let original = parsed.document().header().ok_or("original header")?;
    assert!(HeaderRef::from(&copy).roles().contains("selected"));
    assert!(!original.roles().contains("selected"));
    assert!(!original.title().ok_or("original title")?.is_empty());
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[rstest::rstest]
fn author_copies_allocate_nothing_and_retain_source_text(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("{HEADER}Body.\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let original = parsed
        .document()
        .header()
        .ok_or("header")?
        .authors()
        .next()
        .ok_or("author")?;
    let region = Region::new(ALLOCATOR);
    let mut copy = black_box(Author::from(original));
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.bytes_allocated, 0);
    let expected = Document::from(parsed.document())
        .take_header()
        .ok_or("header")?
        .take_authors()
        .next()
        .ok_or("author")?;
    assert_eq!(copy, expected);
    assert!(ptr::eq(
        AuthorRef::from(&copy).first_name().as_ptr(),
        original.first_name().as_ptr()
    ));
    copy.set_first_name("Changed");
    assert_eq!(AuthorRef::from(&copy).first_name(), "Changed");
    assert_eq!(original.first_name(), "Élodie");
    Ok(())
}

fn copy_cost(body: &str, mode: CompatibilityMode) -> Result<(usize, usize, usize), Error> {
    let source = format!("{HEADER}{body}");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let view = parsed.document().header().ok_or("header")?;
    let region = Region::new(ALLOCATOR);
    let copy = black_box(Header::from(view));
    let stats = region.change();
    assert_eq!(
        copy,
        Document::from(parsed.document())
            .take_header()
            .ok_or("header")?
    );
    Ok((
        stats.allocations,
        stats.reallocations,
        stats.bytes_allocated,
    ))
}

#[rstest::rstest]
fn header_copy_cost_does_not_grow_with_the_document_body(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let small = copy_cost("Body.\n", mode)?;
    let large = copy_cost(&"Unrelated paragraph.\n\n".repeat(1000), mode)?;
    assert!(small.0 > 0);
    assert_eq!(small, large);
    Ok(())
}

#[rstest::rstest]
fn copies_preserve_legacy_title_and_subtitle_presence(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("{HEADER}Body.\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    for title in [Title::default(), Title::new(Vec::new())] {
        for subtitle in [None, Some(Title::default()), Some(Title::new(Vec::new()))] {
            let mut header = Header::from(parsed.document().header().ok_or("header")?);
            header.set_title(title.clone());
            header.set_subtitle(subtitle.clone());
            let mut copy = Header::from(HeaderRef::from(&header));
            assert_eq!(copy, header);
            assert_eq!(copy.take_title().is_present(), title.is_present());
            assert_eq!(
                copy.take_subtitle().as_ref().map(Title::is_present),
                subtitle.as_ref().map(Title::is_present)
            );
        }
    }
    Ok(())
}
