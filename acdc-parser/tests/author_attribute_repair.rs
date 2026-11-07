//! Explicit author repair updates derived fields while retaining assignment intent.

use acdc_parser::{
    AttributeValue, Author, CompatibilityMode, Document, DocumentAttributeAssignment,
    DocumentAttributeValue, Header, Location, Options, Title, ast::DocumentRef, parse,
};

type Error = Box<dyn std::error::Error>;
const MODES: [CompatibilityMode; 2] = [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft];

fn value<'a>(document: DocumentRef<'a>, name: &str) -> Option<&'a str> {
    document
        .attributes()
        .get(name)
        .and_then(DocumentAttributeValue::text)
}

fn replace_authors<'a>(document: &mut Document<'a>, authors: Vec<Author<'a>>) -> Result<(), Error> {
    let mut header = document.take_header().ok_or("header")?;
    header.set_authors(authors);
    document.set_header(Some(header));
    Ok(())
}

#[test]
fn changed_authors_need_explicit_repair_and_do_not_replay_selected_content() -> Result<(), Error> {
    for mode in MODES {
        let parsed = parse(
            "= T\nAlice Smith <old@example.org>; Bob Jones\n:captured: {author}\n:kept: header\n\n:kept: body\n\n{author} / {kept}.\n",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let original = serde_json::to_value(parsed.document())?;
        let mut document = Document::from(parsed.document());
        replace_authors(
            &mut document,
            vec![Author::from_parts(
                "Élodie",
                Some("M"),
                "Durand",
                "prepared",
            )],
        )?;
        assert_eq!(
            value(DocumentRef::from(&document), "author"),
            Some("Alice Smith")
        );
        document.rebuild_author_attributes();
        let read = DocumentRef::from(&document);
        for (name, expected) in [
            ("author", "Élodie M Durand"),
            ("authors", "Élodie M Durand"),
            ("firstname", "Élodie"),
            ("middlename", "M"),
            ("lastname", "Durand"),
            ("authorinitials", "prepared"),
            ("authorcount", "1"),
            ("captured", "Alice Smith"),
            ("kept", "header"),
        ] {
            assert_eq!(value(read, name), Some(expected), "{name}");
        }
        for name in [
            "email",
            "author_2",
            "firstname_2",
            "lastname_2",
            "authorinitials_2",
        ] {
            assert_eq!(value(read, name), None, "{name}");
        }
        let edited = serde_json::to_value(read)?;
        assert_eq!(
            edited.get("blocks").ok_or("edited blocks")?,
            original.get("blocks").ok_or("original blocks")?
        );
        assert_eq!(serde_json::to_value(parsed.document())?, original);
        let repaired = serde_json::to_value(read)?;
        document.rebuild_author_attributes();
        assert_eq!(
            serde_json::to_value(DocumentRef::from(&document))?,
            repaired
        );
    }
    Ok(())
}

#[test]
fn source_overrides_and_unsets_survive_author_replacement_and_removal() -> Result<(), Error> {
    for mode in MODES {
        let parsed = parse(
            "= T\nAlice Smith; Bob Jones\n:firstname: Explicit\n:lastname!:\n:email_2: kept@example.org\n:author-extra: extension\n:author_02: lookalike\n\nBody\n",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let mut document = Document::from(parsed.document());
        replace_authors(
            &mut document,
            vec![Author::from_parts("New", None, "Writer", "NW")],
        )?;
        document.rebuild_author_attributes();
        for remove_header in [false, true] {
            if remove_header {
                let _ = document.take_header();
            } else {
                replace_authors(&mut document, Vec::new())?;
            }
            document.rebuild_author_attributes();
            let read = DocumentRef::from(&document);
            for (name, expected) in [
                ("firstname", "Explicit"),
                ("email_2", "kept@example.org"),
                ("author-extra", "extension"),
                ("author_02", "lookalike"),
                ("authorcount", "0"),
            ] {
                assert_eq!(value(read, name), Some(expected), "{name}");
            }
            assert_eq!(
                read.attributes().assignment("lastname"),
                Some(&DocumentAttributeAssignment::Unset)
            );
            for name in ["author", "authors", "author_2", "firstname_2"] {
                assert_eq!(value(read, name), None, "{name}");
            }
        }
    }
    Ok(())
}

#[test]
fn locked_and_default_caller_fields_remain_distinct_from_generated_fields() -> Result<(), Error> {
    for mode in MODES {
        for locked in [false, true] {
            let builder = Options::builder().with_compatibility_mode(mode);
            let builder = if locked {
                builder
                    .with_attribute("firstname", "Caller")
                    .with_attribute("email", AttributeValue::None)
            } else {
                builder
                    .with_default_attribute("firstname", "Caller")
                    .with_default_attribute("email", AttributeValue::None)
            };
            let parsed = parse("= T\nAlice Smith\n\nBody\n", &builder.build()?)?;
            let mut document = Document::from(parsed.document());
            let mut author = Author::from_parts("New", None, "Writer", "NW");
            author.set_email(Some("new@example.org"));
            replace_authors(&mut document, vec![author])?;
            document.rebuild_author_attributes();
            let read = DocumentRef::from(&document);
            assert_eq!(value(read, "firstname"), Some("Caller"));
            assert_eq!(value(read, "lastname"), Some("Writer"));
            assert_eq!(
                read.attributes().assignment("email"),
                Some(&DocumentAttributeAssignment::Unset)
            );
        }
    }
    Ok(())
}

#[test]
fn adding_prepared_authors_overrides_only_the_implicit_count_default() -> Result<(), Error> {
    for count in [None, Some("77")] {
        let mut document = Document::default();
        if let Some(count) = count {
            document.set_attributes(
                Options::with_attributes([("authorcount", count), ("kept", "caller")])?
                    .into_document_attributes(),
            );
        }
        document.set_header(Some(
            Header::new(Title::default(), Location::default()).with_authors(vec![
                Author::from_parts("One", None, "", "1"),
                Author::from_parts("Two", None, "", "2"),
            ]),
        ));
        document.rebuild_author_attributes();
        let read = DocumentRef::from(&document);
        assert_eq!(value(read, "authorcount"), Some(count.unwrap_or("2")));
        assert_eq!(value(read, "authors"), Some("One, Two"));
        assert_eq!(value(read, "author_2"), Some("Two"));
        if count.is_some() {
            assert_eq!(value(read, "kept"), Some("caller"));
        }
    }
    Ok(())
}

#[test]
fn prepared_empty_parts_and_optional_values_are_not_normalized() -> Result<(), Error> {
    let mut author = Author::from_parts("", Some(""), "", "supplied");
    author.set_email(Some(""));
    let mut document = Document::default();
    document.set_header(Some(
        Header::new(Title::default(), Location::default()).with_authors(vec![author]),
    ));
    document.rebuild_author_attributes();
    let read = DocumentRef::from(&document);
    for (name, expected) in [
        ("author", " "),
        ("firstname", ""),
        ("middlename", ""),
        ("lastname", ""),
        ("authorinitials", "supplied"),
        ("email", ""),
        ("authorcount", "1"),
    ] {
        assert_eq!(value(read, name), Some(expected), "{name}");
    }
    replace_authors(
        &mut document,
        vec![Author::from_parts("Only", None, "", "O")],
    )?;
    document.rebuild_author_attributes();
    let read = DocumentRef::from(&document);
    assert_eq!(value(read, "middlename"), None);
    assert_eq!(value(read, "email"), None);
    assert_eq!(value(read, "lastname"), Some(""));
    Ok(())
}

#[test]
fn unchanged_repair_keeps_header_and_other_catalogs_intact() -> Result<(), Error> {
    for mode in MODES {
        let parsed = parse(
            "= T\nAlice Smith\n\n== Section\n\nfootnote:n[Note].\n",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let mut document = Document::from(parsed.document());
        let original = serde_json::to_value(DocumentRef::from(&document))?;
        let refs: std::collections::HashMap<_, _> = document.take_references().collect();
        document.set_references(refs.clone());
        let notes: Vec<_> = document.take_footnotes().collect();
        document.set_footnotes(notes.clone());
        let toc: Vec<_> = document.take_toc_entries().collect();
        document.set_toc_entries(toc.clone());
        document.rebuild_author_attributes();
        assert_eq!(
            serde_json::to_value(DocumentRef::from(&document))?,
            original
        );
        assert_eq!(
            document
                .take_references()
                .collect::<std::collections::HashMap<_, _>>(),
            refs
        );
        assert_eq!(document.take_footnotes().collect::<Vec<_>>(), notes);
        assert_eq!(document.take_toc_entries().collect::<Vec<_>>(), toc);
    }
    Ok(())
}

#[test]
fn prepared_repair_retains_an_explicitly_unset_count() -> Result<(), Error> {
    for locked in [false, true] {
        let builder = Options::builder();
        let options = if locked {
            builder.with_attribute("authorcount", AttributeValue::None)
        } else {
            builder.with_default_attribute("authorcount", AttributeValue::None)
        }
        .build()?;
        let mut document = Document::default();
        document.set_attributes(options.into_document_attributes());
        document.set_header(Some(
            Header::new(Title::default(), Location::default())
                .with_authors(vec![Author::from_parts("New", None, "Writer", "NW")]),
        ));
        let before = DocumentRef::from(&document)
            .attributes()
            .get("authorcount")
            .map(|value| value.clone().into_static());
        document.rebuild_author_attributes();
        let read = DocumentRef::from(&document);
        assert_eq!(read.attributes().get("authorcount"), before.as_ref());
        assert_eq!(
            read.attributes().assignment("authorcount"),
            Some(&DocumentAttributeAssignment::Unset)
        );
        assert_eq!(value(read, "author"), Some("New Writer"));
    }
    Ok(())
}

#[test]
fn retained_author_assignments_do_not_reparse_prepared_header_names() -> Result<(), Error> {
    for mode in MODES {
        for caller in [false, true] {
            let builder = Options::builder().with_compatibility_mode(mode);
            let options = if caller {
                builder.with_attribute("author", "Caller Writer")
            } else {
                builder
            }
            .build()?;
            let parsed = parse(
                "= T\nAlice Smith\n:author: Explicit Writer\n:authors: Custom list\n\nBody\n",
                &options,
            )?;
            let mut document = Document::from(parsed.document());
            replace_authors(
                &mut document,
                vec![Author::from_parts("New", None, "Name", "supplied")],
            )?;
            document.rebuild_author_attributes();
            let read = DocumentRef::from(&document);
            assert_eq!(
                value(read, "author"),
                Some(if caller {
                    "Caller Writer"
                } else {
                    "Explicit Writer"
                })
            );
            assert_eq!(value(read, "authors"), Some("Custom list"));
            assert_eq!(value(read, "firstname"), Some("New"));
            assert_eq!(value(read, "authorinitials"), Some("supplied"));
            assert_eq!(
                read.header()
                    .ok_or("header")?
                    .authors()
                    .next()
                    .ok_or("author")?
                    .first_name(),
                "New"
            );
        }
    }
    Ok(())
}
