//! Header edits transfer owned content and preserve declaration snapshots.

use acdc_parser::{
    Author, CompatibilityMode, Document, Header, Location, Options, Title,
    ast::{AuthorRef, ContentRef, DocumentRef, ElementIdRef, HeaderRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

const SOURCE: &str = "[#heading.hero%featured]\n= *Heading*: Subtitle\nÉlodie Marie Durand <elodie@example.org>\n\nBody.\n";

fn transfer(mut header: Header<'_>) -> Result<Header<'_>, Error> {
    let location = Location::from(HeaderRef::from(&header).location());
    let metadata = header.take_metadata();
    let title = header.take_title();
    let subtitle = header.take_subtitle();
    let authors = header.take_authors();
    let read = HeaderRef::from(&header);
    assert!(read.title().is_none());
    assert!(read.subtitle().is_none());
    assert_eq!(read.authors().count(), 0);
    assert!(read.roles().is_empty());
    drop(header);

    let mut rebuilt = Header::new(title, Location::default());
    rebuilt.set_metadata(metadata)?;
    rebuilt.set_subtitle(subtitle);
    rebuilt.set_authors(authors);
    rebuilt.set_location(location);
    Ok(rebuilt)
}

#[rstest::rstest]
fn parts_outlive_the_header_and_keep_exact_prepared_values(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let original = Document::from(parsed.document())
        .take_header()
        .ok_or("header")?;
    let expected = Document::from(parsed.document())
        .take_header()
        .ok_or("header")?;
    let rebuilt = transfer(original)?;
    assert_eq!(rebuilt, expected);
    assert_eq!(
        serde_json::to_value(&rebuilt)?,
        serde_json::to_value(&expected)?
    );
    let read = HeaderRef::from(&rebuilt);
    assert_eq!(read.id().map(ElementIdRef::as_str), Some("heading"));
    assert!(read.roles().contains("hero"));
    assert!(read.options().contains("featured"));
    Ok(())
}

#[rstest::rstest]
fn rejected_metadata_leaves_the_entire_header_unchanged(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut header = Document::from(parsed.document())
        .take_header()
        .ok_or("header")?;
    let expected = Document::from(parsed.document())
        .take_header()
        .ok_or("header")?;
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
        let mut invalid = header.clone().take_metadata();
        invalid.attributes.set(name.into(), "shadow".into());
        assert!(header.set_metadata(invalid).is_err(), "{name}");
        assert_eq!(header, expected, "{name}");
    }
    Ok(())
}

#[rstest::rstest]
fn title_and_subtitle_presence_survive_replacement_and_transfer(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut header = Document::from(parsed.document())
        .take_header()
        .ok_or("header")?;
    header.set_title(Title::new(Vec::new()));
    header.set_subtitle(Some(Title::default()));
    let rebuilt = transfer(header)?;
    let read = HeaderRef::from(&rebuilt);
    assert!(read.title().is_some_and(ContentRef::is_empty));
    assert!(read.subtitle().is_some_and(ContentRef::is_empty));
    let mut rebuilt = rebuilt;
    assert!(rebuilt.take_title().is_present());
    assert!(rebuilt.take_subtitle().is_some());
    rebuilt.set_title(Title::default());
    rebuilt.set_subtitle(None);
    assert!(HeaderRef::from(&rebuilt).title().is_none());
    assert!(HeaderRef::from(&rebuilt).subtitle().is_none());
    Ok(())
}

#[rstest::rstest]
fn author_edits_do_not_replay_author_derived_document_attributes(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let mut document = Document::from(parsed.document());
    let mut header = document.take_header().ok_or("header")?;
    let mut authors = header.take_authors();
    assert_eq!(authors.len(), 1);
    let mut author = authors.next().ok_or("author")?;
    author.set_first_name("New");
    author.set_middle_name(None);
    author.set_last_name("Name");
    author.set_initials("NN");
    author.set_email(None);
    let read = AuthorRef::from(&author);
    assert_eq!(read.first_name(), "New");
    assert_eq!(read.middle_name(), None);
    assert_eq!(read.last_name(), "Name");
    assert_eq!(read.initials(), "NN");
    assert_eq!(read.email(), None);
    header.set_authors([author]);
    document.set_header(Some(header));
    let read = DocumentRef::from(&document);
    assert_eq!(
        read.header()
            .ok_or("edited header")?
            .authors()
            .next()
            .ok_or("edited author")?
            .first_name(),
        "New"
    );
    assert_eq!(
        read.attributes()
            .get("author")
            .and_then(|value| value.text()),
        Some("Élodie Marie Durand")
    );
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[test]
fn author_setters_keep_display_ready_text_without_normalization() {
    let mut author = Author::from_parts("First", None, "Last", "FL");
    author.set_first_name(" First ");
    author.set_middle_name(Some("Middle"));
    author.set_last_name("Last-Name");
    author.set_initials("F.M.");
    author.set_email(Some("person+tag@example.org"));
    let read = AuthorRef::from(&author);
    assert_eq!(read.first_name(), " First ");
    assert_eq!(read.middle_name(), Some("Middle"));
    assert_eq!(read.last_name(), "Last-Name");
    assert_eq!(read.initials(), "F.M.");
    assert_eq!(read.email(), Some("person+tag@example.org"));
}

fn mark(mut header: Header<'_>) -> Result<Header<'_>, Error> {
    let mut metadata = header.clone().take_metadata();
    metadata.roles.push("selected");
    header.set_metadata(metadata)?;
    Ok(header)
}

#[rstest::rstest]
fn cloned_headers_keep_source_lifetimes_for_prepared_metadata_edits(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let header = Document::from(parsed.document())
        .take_header()
        .ok_or("header")?;
    let marked = mark(header.clone())?;
    assert!(HeaderRef::from(&marked).roles().contains("selected"));
    assert!(!HeaderRef::from(&header).roles().contains("selected"));
    assert_eq!(
        HeaderRef::from(&marked).id().map(ElementIdRef::as_str),
        HeaderRef::from(&header).id().map(ElementIdRef::as_str)
    );
    Ok(())
}
