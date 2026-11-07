//! Prepared index labels survive edits and collection visits authored occurrences once.

use acdc_parser::{
    CompatibilityMode, Document, IndexTerm, IndexTermKind, IndexTermRelationship, InlineNode,
    Location, Options, Plain, SafeMode,
    ast::{ContentRef, DocumentRef, IndexTermRef, IndexTermRelationshipRef, InlineKind},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;
const MODES: [CompatibilityMode; 2] = [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft];

fn nodes(text: &'static str) -> Vec<InlineNode<'static>> {
    vec![InlineNode::PlainText(Plain::new(text, Location::default()))]
}

fn text(content: ContentRef<'_>) -> Result<String, Error> {
    content.inlines().try_fold(String::new(), |mut text, node| {
        let InlineKind::Plain(plain) = node.kind() else {
            return Err("expected plain label".into());
        };
        text.push_str(plain.text());
        Ok(text)
    })
}

fn terms(document: DocumentRef<'_>) -> Vec<IndexTermRef<'_>> {
    document
        .content_occurrences()
        .flat_map(|field| field.content().inlines())
        .filter_map(|node| {
            if let InlineKind::IndexTerm(term) = node.kind() {
                Some(term)
            } else {
                None
            }
        })
        .collect()
}

fn labels(document: DocumentRef<'_>) -> Result<Vec<String>, Error> {
    terms(document)
        .into_iter()
        .map(|term| text(term.catalog().term()))
        .collect()
}

#[test]
fn editing_an_equal_source_label_keeps_its_initial_registration() -> Result<(), Error> {
    for mode in MODES {
        let parsed = parse(
            "indexterm2:[Original]",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let term = terms(parsed.document()).into_iter().next().ok_or("term")?;
        let mut copied = IndexTerm::from(term);
        copied.replace_kind(IndexTermKind::Flow(nodes("Changed")));
        let read = IndexTermRef::from(&copied);
        assert_eq!(text(read.term())?, "Changed");
        assert_eq!(text(read.catalog().term())?, "Original");
        assert_eq!(text(term.catalog().term())?, "Original");
    }
    Ok(())
}

#[test]
fn caller_prepared_labels_also_define_a_stable_initial_catalog() -> Result<(), Error> {
    let mut term = IndexTerm::new(IndexTermKind::Flow(nodes("Prepared")), Location::default());
    term.replace_kind(IndexTermKind::Flow(nodes("Changed")));
    assert_eq!(text(IndexTermRef::from(&term).term())?, "Changed");
    assert_eq!(
        text(IndexTermRef::from(&term).catalog().term())?,
        "Prepared"
    );
    Ok(())
}

#[test]
fn hierarchy_presence_survives_changing_the_display_form() -> Result<(), Error> {
    let mut term = IndexTerm::new(
        IndexTermKind::Concealed {
            term: nodes("Root"),
            secondary: Some(Vec::new()),
            tertiary: None,
        },
        Location::default(),
    );
    term.replace_kind(IndexTermKind::Flow(nodes("Visible")));
    let read = IndexTermRef::from(&term);
    assert!(read.is_visible());
    assert!(read.secondary().is_none());
    let catalog = read.catalog();
    assert_eq!(text(catalog.term())?, "Root");
    assert!(
        catalog
            .secondary()
            .ok_or("present empty secondary")?
            .is_empty()
    );
    assert!(catalog.tertiary().is_none());
    Ok(())
}

#[test]
fn removing_or_replacing_a_relationship_keeps_its_registered_target() -> Result<(), Error> {
    for mode in MODES {
        let parsed = parse(
            "indexterm:[Root,see=Target]",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let term = terms(parsed.document()).into_iter().next().ok_or("term")?;
        for replace in [false, true] {
            let mut copied = IndexTerm::from(term);
            if replace {
                copied.replace_relationship(Some(IndexTermRelationship::SeeAlso {
                    targets: vec![nodes("Changed")],
                }));
            } else {
                assert!(copied.take_relationship().is_some());
            }
            let Some(IndexTermRelationshipRef::See { target }) =
                IndexTermRef::from(&copied).catalog().relationship()
            else {
                return Err("registered see target".into());
            };
            assert_eq!(text(target)?, "Target");
        }
    }
    Ok(())
}

#[test]
fn no_op_and_restored_edits_keep_owned_equality_and_native_json_elision() -> Result<(), Error> {
    for source in [
        "indexterm2:[Original]",
        "[subs=\"macros,replacements\"]\nindexterm2:[Copyright (C)]",
    ] {
        let parsed = parse(source, &Options::default())?;
        let read = terms(parsed.document()).into_iter().next().ok_or("term")?;
        let mut copied = IndexTerm::from(read);
        let original = copied.clone();
        let before = serde_json::to_value(&copied)?;
        let initial_kind = copied.replace_kind(IndexTermKind::Flow(nodes("Changed")));
        copied.replace_kind(initial_kind);
        assert_eq!(copied, original);
        let same_kind = IndexTerm::from(read).replace_kind(IndexTermKind::Flow(Vec::new()));
        copied.replace_kind(same_kind);
        let relationship = copied.replace_relationship(Some(IndexTermRelationship::See {
            target: nodes("Target"),
        }));
        assert!(copied.take_relationship().is_some());
        copied.replace_relationship(relationship);
        assert_eq!(copied, original);
        assert_eq!(serde_json::to_value(&copied)?, before);
    }
    Ok(())
}

#[test]
fn collection_follows_tree_membership_without_visiting_reference_or_footnote_copies()
-> Result<(), Error> {
    for mode in MODES {
        let parsed = parse(
            "= Document ((Header))\n:label: Registered\n\n[#paragraph]\n.Title ((Caption))\nBody ((Body {label})) footnote:[((Note))].\n",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        assert_eq!(
            labels(parsed.document())?,
            ["Header", "Caption", "Body Registered", "Note"]
        );
        let mut document = Document::from(parsed.document());
        let mut blocks: Vec<_> = document.take_blocks().collect();
        let body = blocks.pop().ok_or("paragraph")?;
        document.set_blocks([]);
        assert_eq!(labels(DocumentRef::from(&document))?, ["Header"]);
        assert!(
            DocumentRef::from(&document)
                .references()
                .contains("paragraph")
        );
        assert_eq!(DocumentRef::from(&document).footnotes().count(), 1);
        document.set_blocks([body.clone(), body]);
        document.set_attributes(
            Options::with_attributes([("label", "Wrong")])?.into_document_attributes(),
        );
        assert_eq!(
            labels(DocumentRef::from(&document))?,
            [
                "Header",
                "Caption",
                "Body Registered",
                "Note",
                "Caption",
                "Body Registered",
                "Note"
            ]
        );
    }
    Ok(())
}

#[test]
fn selected_repeated_includes_and_asciidoc_cells_keep_separate_occurrences() -> Result<(), Error> {
    for mode in MODES {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("main.adoc");
        std::fs::write(
            directory.path().join("part.adoc"),
            "((Unused))\n\n// tag::selected[]\n((Included {label}))\n// end::selected[]\n",
        )?;
        std::fs::write(
            &path,
            "= T ((Header))\n:label: First\n\ninclude::part.adoc[tag=selected]\n\n:label: Second\n\ninclude::part.adoc[tag=selected]\n\n[cols=a]\n|===\n|Cell ((Cell {label}))\n|===\n",
        )?;
        let parsed = parse_file(
            &path,
            &Options::builder()
                .with_safe_mode(SafeMode::Unsafe)
                .with_compatibility_mode(mode)
                .build()?,
        )?;
        let mut document = Document::from(parsed.document());
        document.set_attributes(
            Options::with_attributes([("label", "Wrong")])?.into_document_attributes(),
        );
        assert_eq!(
            labels(DocumentRef::from(&document))?,
            ["Header", "Included First", "Included Second", "Cell Second"]
        );
    }
    Ok(())
}

#[test]
fn an_index_term_kept_only_in_a_catalog_is_not_an_authored_occurrence() -> Result<(), Error> {
    let inner = IndexTerm::new(IndexTermKind::Flow(nodes("Inner")), Location::default());
    let mut outer = IndexTerm::new(
        IndexTermKind::Flow(vec![InlineNode::Macro(
            acdc_parser::InlineMacro::IndexTerm(Box::new(inner)),
        )]),
        Location::default(),
    );
    outer.replace_kind(IndexTermKind::Flow(nodes("Displayed")));
    let mut document = Document::default();
    document.set_blocks([acdc_parser::Block::Paragraph(acdc_parser::Paragraph::new(
        vec![InlineNode::Macro(acdc_parser::InlineMacro::IndexTerm(
            Box::new(outer),
        ))],
        Location::default(),
    ))]);
    let occurrences = terms(DocumentRef::from(&document));
    assert_eq!(occurrences.len(), 1);
    let outer = occurrences.first().ok_or("outer")?;
    assert_eq!(text(outer.term())?, "Displayed");
    assert!(matches!(
        outer
            .catalog()
            .term()
            .inlines()
            .next()
            .ok_or("catalog child")?
            .kind(),
        InlineKind::IndexTerm(_)
    ));
    Ok(())
}
