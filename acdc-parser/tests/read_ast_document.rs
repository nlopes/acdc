use acdc_parser::{
    Author, CompatibilityMode, Document, DocumentAttributeAssignment, DocumentAttributeValue,
    Header, Location, Options, SafeMode, Title,
    ast::{
        AttributeValueRef, AuthorRef, BlockKind, ContentRef, DocumentRef, HeaderRef, InlineKind,
    },
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

fn text(content: ContentRef<'_>) -> Result<String, Error> {
    let mut result = String::new();
    for inline in content.inlines() {
        if let InlineKind::Plain(node) = inline.kind() {
            result.push_str(node.text());
        } else if let InlineKind::Bold(node) = inline.kind() {
            result.push_str(&text(node.content())?);
        } else if let InlineKind::Raw(node) = inline.kind() {
            result.push_str(node.text());
        } else {
            return Err("unexpected test content".into());
        }
    }
    Ok(result)
}

#[rstest::rstest]
fn header_reads_keep_title_subtitle_authors_and_revision_values(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[#book.role,options=custom]\n= Main: *Subtitle*\nÉlodie Marie Durand <elodie@example.org>; José García\nv2.5, 2026-10-29: Reviewed\n\nBody.\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let document = parsed.document_ref();
    let header = document.header().ok_or("header")?;
    assert_eq!(text(header.title().ok_or("title")?)?, "Main");
    assert_eq!(header.title(), document.title());
    assert_eq!(text(header.subtitle().ok_or("subtitle")?)?, "Subtitle");
    assert_eq!(header.id().ok_or("id")?.as_str(), "book");
    assert!(header.roles().contains("role"));
    assert!(header.options().contains("custom"));
    assert_eq!(
        header.attributes().get("title"),
        header.title().map(AttributeValueRef::Content)
    );
    let mut authors = header.authors();
    let first = authors.next().ok_or("first author")?;
    assert_eq!(first.first_name(), "Élodie");
    assert_eq!(first.middle_name(), Some("Marie"));
    assert_eq!(first.last_name(), "Durand");
    assert_eq!(first.initials(), "ÉMD");
    assert_eq!(first.email(), Some("elodie@example.org"));
    let second = authors.next().ok_or("second author")?;
    assert_eq!(second.first_name(), "José");
    assert_eq!(second.middle_name(), None);
    assert_eq!(second.email(), None);
    assert!(authors.next().is_none());
    for (name, expected) in [
        ("revnumber", "2.5"),
        ("revdate", "2026-10-29"),
        ("revremark", "Reviewed"),
    ] {
        assert_eq!(
            document
                .attributes()
                .get(name)
                .and_then(DocumentAttributeValue::text),
            Some(expected)
        );
    }
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
fn document_values_keep_defaults_explicit_unsets_and_their_own_namespace(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        ":flag:\n:removed!:\n:role: single\n:roles: plural\n:options: document options\n\nText.\n",
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_attribute("max-include-depth", "064")
            .with_attribute("empty", "")
            .build()?,
    )?;
    let attributes = parsed.document_ref().attributes();
    assert!(attributes.get("flag").ok_or("flag")?.is_presence());
    assert!(matches!(
        attributes.assignment("removed"),
        Some(DocumentAttributeAssignment::Unset)
    ));
    assert!(attributes.is_explicit("removed"));
    assert!(!attributes.contains("removed"));
    assert!(attributes.assignment("missing").is_none());
    assert!(attributes.contains("figure-caption"));
    assert!(!attributes.is_explicit("figure-caption"));
    assert_eq!(
        attributes
            .get("role")
            .and_then(DocumentAttributeValue::text),
        Some("single")
    );
    assert_eq!(
        attributes
            .get("roles")
            .and_then(DocumentAttributeValue::text),
        Some("plural")
    );
    assert!(attributes.get("opts").is_none());
    assert_eq!(
        attributes
            .get("options")
            .and_then(DocumentAttributeValue::text),
        Some("document options")
    );
    assert_eq!(
        attributes
            .get("empty")
            .and_then(DocumentAttributeValue::as_str),
        Some("")
    );
    let depth = attributes.get("max-include-depth").ok_or("depth")?;
    assert_eq!(depth.as_integer(), Some(64));
    assert_eq!(depth.as_str(), None);
    assert_eq!(depth.text(), Some("064"));
    let mut written = String::new();
    depth.write_text(&mut written)?;
    assert_eq!(written, "064");
    assert!(!attributes.is_empty());
    for (name, value) in attributes.iter() {
        assert_eq!(attributes.get(name), Some(value));
    }
    assert!(!attributes.iter().any(|(name, _)| name == "removed"));
    assert!(
        attributes
            .assignments()
            .any(|(name, value)| name == "removed"
                && matches!(value, DocumentAttributeAssignment::Unset))
    );
    Ok(())
}

#[rstest::rstest]
fn snapshots_stay_fixed_while_accepted_body_assignments_replay_in_order(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= T\n:state: header\n\nBefore.\n\n:locked: rejected\n:state: body\n\nAfter.\n\n:state!:\n\nUnset.\n",
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_attribute("locked", "caller")
            .build()?,
    )?;
    let document = parsed.document_ref();
    let attributes = document.attributes();
    let mut active = attributes.get("state");
    let mut paragraphs = Vec::new();
    let mut events = Vec::new();
    for block in document.blocks() {
        if let BlockKind::DocumentAttribute(event) = block.kind() {
            assert_eq!(event.name(), "state");
            active = event.assignment().value();
            events.push(event.location().start().line());
        } else if let BlockKind::Paragraph(_) = block.kind() {
            paragraphs.push(active.and_then(DocumentAttributeValue::text));
        } else {
            return Err("unexpected body block".into());
        }
    }
    assert_eq!(paragraphs, [Some("header"), Some("body"), None]);
    assert_eq!(events, [7, 11]);
    assert_eq!(
        attributes
            .get("state")
            .and_then(DocumentAttributeValue::text),
        Some("header")
    );
    assert_eq!(
        attributes
            .get("locked")
            .and_then(DocumentAttributeValue::text),
        Some("caller")
    );
    Ok(())
}

#[rstest::rstest]
fn cell_initial_assignments_are_ordered_and_stay_in_the_cell_scope(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= T\n:doctype: book\n:toc:\n:showtitle:\n\n[cols=a]\n|===\n|\n:local: cell\n\n{doctype}\n|===\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let document = parsed.document_ref();
    let BlockKind::Table(table) = document.blocks().next().ok_or("table")?.kind() else {
        return Err("table".into());
    };
    let cell = table
        .rows()
        .next()
        .ok_or("row")?
        .cells()
        .next()
        .ok_or("cell")?;
    let mut doctype = document.attributes().get("doctype");
    let mut toc = document.attributes().get("toc");
    for (name, assignment) in cell.initial_attributes() {
        match name {
            "doctype" => doctype = assignment.value(),
            "toc" => toc = assignment.value(),
            _ => {}
        }
    }
    assert_eq!(
        doctype.and_then(DocumentAttributeValue::text),
        Some("article")
    );
    assert_eq!(toc, None);
    let mut local = None;
    for block in cell.blocks() {
        if let BlockKind::DocumentAttribute(event) = block.kind() {
            if event.name() == "local" {
                local = event.assignment().value();
            }
        } else if let BlockKind::Paragraph(paragraph) = block.kind() {
            assert_eq!(text(paragraph.content())?, "article");
        }
    }
    assert_eq!(local.and_then(DocumentAttributeValue::text), Some("cell"));
    assert_eq!(
        document
            .attributes()
            .get("doctype")
            .and_then(DocumentAttributeValue::text),
        Some("book")
    );
    assert!(document.attributes().contains("toc"));
    assert!(document.attributes().get("local").is_none());
    let owner_table = parsed.document().blocks.first().ok_or("owner table")?;
    let acdc_parser::Block::DelimitedBlock(block) = owner_table else {
        return Err("delimited table".into());
    };
    let acdc_parser::DelimitedBlockType::DelimitedTable(owner) = &block.inner else {
        return Err("table owner".into());
    };
    let expected = owner
        .rows
        .first()
        .ok_or("owner row")?
        .columns
        .first()
        .ok_or("owner cell")?
        .initial_attributes();
    assert!(cell.initial_attributes().eq(expected));
    Ok(())
}

#[rstest::rstest]
fn included_assignments_keep_authored_unicode_coordinates(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let main = directory.path().join("main.adoc");
    let include = directory.path().join("body.adoc");
    std::fs::write(&main, "= T\n:enabled:\n\ninclude::body.adoc[]\n")?;
    std::fs::write(
        &include,
        "ifdef::enabled[]\n:value: café\n\n{value}\nendif::[]\n",
    )?;
    let parsed = parse_file(
        &main,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let mut blocks = parsed.document_ref().blocks();
    let BlockKind::DocumentAttribute(event) = blocks.next().ok_or("event")?.kind() else {
        return Err("attribute event".into());
    };
    assert_eq!(event.name(), "value");
    assert_eq!(
        event
            .assignment()
            .value()
            .and_then(DocumentAttributeValue::text),
        Some("café")
    );
    let location = parsed.resolve_location(event.location());
    assert_eq!(location.file, Some(include));
    assert_eq!(
        (location.location.start.line, location.location.start.column),
        (2, 1)
    );
    let BlockKind::Paragraph(paragraph) = blocks.next().ok_or("paragraph")?.kind() else {
        return Err("paragraph".into());
    };
    assert_eq!(text(paragraph.content())?, "café");
    assert!(blocks.next().is_none());
    Ok(())
}

#[test]
fn constructed_headers_keep_optional_content_and_shared_authors() -> Result<(), Error> {
    fn shared<T: Copy + Send + Sync>(_: T) {}
    let author = Author::from_parts("Ada", None, "Lovelace", "AL");
    assert_eq!(AuthorRef::from(&author).last_name(), "Lovelace");
    let mut header = Header::new(Title::default(), Location::default());
    assert!(HeaderRef::from(&header).title().is_none());
    assert!(HeaderRef::from(&header).subtitle().is_none());
    header.title = Title::new(Vec::new());
    header.subtitle = Some(Title::default());
    header.authors.push(author);
    let header = HeaderRef::from(&header);
    assert!(header.title().ok_or("empty title")?.is_empty());
    assert!(header.subtitle().ok_or("empty subtitle")?.is_empty());
    shared(header);
    std::thread::scope(|scope| {
        let authors = header.authors();
        scope.spawn(move || assert_eq!(authors.count(), 1));
    });
    let document = Document::default();
    let document = DocumentRef::from(&document);
    assert!(document.header().is_none());
    let attributes = document.attributes();
    shared(attributes);
    std::thread::scope(|scope| {
        let values = attributes.iter();
        scope.spawn(move || assert!(values.count() > 0));
    });
    Ok(())
}
