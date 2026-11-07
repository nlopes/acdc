use acdc_parser::{
    Block, CompatibilityMode, Document, Footnote, FootnoteDefinition, FootnoteRebuildError,
    InlineMacro, InlineNode, Location, Options, Paragraph, Plain,
    ast::{BlockKind, DocumentRef, FootnoteDefinitionRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn note<'a>(id: Option<&'a str>, body: Option<Vec<InlineNode<'a>>>, number: u32) -> InlineNode<'a> {
    let mut note = Footnote::reference(id, number, Location::default());
    note.replace_content(body);
    InlineNode::Macro(InlineMacro::Footnote(note))
}

fn plain(text: &str) -> InlineNode<'_> {
    InlineNode::PlainText(Plain::new(text, Location::default()))
}

fn document(inlines: Vec<InlineNode<'_>>) -> Document<'_> {
    let paragraph = Paragraph::new(inlines, Location::default());
    let mut document = Document::default();
    document.set_blocks([Block::Paragraph(paragraph)]);
    document
}

fn catalog(document: &Document<'_>) -> Vec<(Option<String>, u32)> {
    DocumentRef::from(document)
        .footnotes()
        .map(|note| (note.id().map(str::to_owned), note.number()))
        .collect()
}

#[rstest::rstest]
fn removing_defining_blocks_promotes_surviving_references_and_drops_unused_catalog_entries(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "footnote:keep[*Retained*.] footnote:drop[Removed.]\n\nfootnote:keep[] footnote:new[New.]",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut edited = Document::from(parsed.document());
    let blocks: Vec<_> = edited.take_blocks().skip(1).collect();
    edited.set_blocks(blocks);
    edited.rebuild_footnotes()?;
    assert_eq!(
        catalog(&edited),
        [(Some("keep".to_owned()), 1), (Some("new".to_owned()), 2)]
    );
    let BlockKind::Paragraph(paragraph) = DocumentRef::from(&edited)
        .blocks()
        .next()
        .ok_or("paragraph")?
        .kind()
    else {
        return Err("paragraph".into());
    };
    let first = paragraph
        .content()
        .inlines()
        .find_map(|inline| {
            if let InlineKind::Footnote(note) = inline.kind() {
                Some(note)
            } else {
                None
            }
        })
        .ok_or("promoted note")?;
    assert!(first.content().is_some());
    assert_eq!(first.location().start().line(), 3);
    let definition = DocumentRef::from(&edited)
        .footnotes()
        .next()
        .ok_or("retained definition")?;
    assert_eq!(definition.location().start().line(), 1);
    let before = serde_json::to_value(DocumentRef::from(&edited))?;
    edited.rebuild_footnotes()?;
    assert_eq!(serde_json::to_value(DocumentRef::from(&edited))?, before);
    Ok(())
}

#[test]
fn forward_references_empty_definitions_and_duplicate_bodies_use_one_selected_body()
-> Result<(), Error> {
    let mut edited = document(vec![
        note(Some("later"), None, 0),
        note(Some("later"), Some(Vec::new()), 17),
        note(
            Some("later"),
            Some(vec![note(
                Some("discarded"),
                Some(vec![plain("unused")]),
                0,
            )]),
            9,
        ),
    ]);
    edited.rebuild_footnotes()?;
    assert_eq!(catalog(&edited), [(Some("later".to_owned()), 1)]);
    let json = serde_json::to_value(DocumentRef::from(&edited))?;
    assert_eq!(
        json.pointer("/blocks/0/inlines/0/inlines"),
        Some(&serde_json::json!([]))
    );
    assert!(json.pointer("/blocks/0/inlines/1/inlines").is_none());
    assert!(json.pointer("/blocks/0/inlines/2/inlines").is_none());
    assert_eq!(
        DocumentRef::from(&edited)
            .footnotes()
            .next()
            .ok_or("empty definition")?
            .content()
            .inlines()
            .count(),
        0
    );
    Ok(())
}

#[test]
fn nested_definitions_are_numbered_before_their_parent_and_references_do_not_repeat_bodies()
-> Result<(), Error> {
    let mut edited = document(vec![
        note(
            Some("outer"),
            Some(vec![
                plain("body"),
                note(Some("inner"), Some(vec![plain("inside")]), 7),
            ]),
            9,
        ),
        note(Some("inner"), None, 7),
    ]);
    edited.rebuild_footnotes()?;
    assert_eq!(
        catalog(&edited),
        [(Some("inner".to_owned()), 1), (Some("outer".to_owned()), 2)]
    );
    let json = serde_json::to_value(DocumentRef::from(&edited))?;
    assert!(json.pointer("/blocks/0/inlines/1/inlines").is_none());
    Ok(())
}

#[rstest::rstest]
#[case(false)]
#[case(true)]
fn missing_and_cyclic_definitions_leave_the_document_unchanged(
    #[case] cycle: bool,
) -> Result<(), Error> {
    let body = cycle.then(|| vec![note(Some("missing"), None, 7)]);
    let mut edited = document(vec![
        note(Some("good"), Some(vec![plain("body")]), 90),
        note(Some("missing"), body, 7),
    ]);
    let before = serde_json::to_value(DocumentRef::from(&edited))?;
    let error = edited
        .rebuild_footnotes()
        .err()
        .ok_or("expected binding error")?;
    if cycle {
        assert!(matches!(
            error,
            FootnoteRebuildError::CyclicDefinition { .. }
        ));
    } else {
        assert!(matches!(
            error,
            FootnoteRebuildError::MissingDefinition { .. }
        ));
    }
    assert_eq!(serde_json::to_value(DocumentRef::from(&edited))?, before);
    assert_eq!(DocumentRef::from(&edited).footnotes().count(), 0);
    Ok(())
}

#[test]
fn anonymous_forward_references_bind_unique_old_numbers_but_reject_ambiguous_numbers()
-> Result<(), Error> {
    let mut edited = document(vec![
        note(None, None, 41),
        note(None, Some(vec![plain("body")]), 41),
    ]);
    edited.rebuild_footnotes()?;
    assert_eq!(catalog(&edited), [(None, 1)]);
    let mut edited = document(vec![
        note(None, Some(vec![plain("first")]), 0),
        note(None, Some(vec![plain("second")]), 0),
    ]);
    edited.rebuild_footnotes()?;
    assert_eq!(catalog(&edited), [(None, 1), (None, 2)]);
    let mut edited = document(vec![
        note(None, Some(vec![plain("first")]), 0),
        note(None, Some(vec![plain("second")]), 0),
        note(None, None, 0),
    ]);
    let before = serde_json::to_value(DocumentRef::from(&edited))?;
    assert!(matches!(
        edited.rebuild_footnotes(),
        Err(FootnoteRebuildError::AmbiguousDefinition { .. })
    ));
    assert_eq!(serde_json::to_value(DocumentRef::from(&edited))?, before);
    Ok(())
}

#[test]
fn prepared_catalog_bodies_support_references_without_a_defining_occurrence() -> Result<(), Error> {
    let mut edited = document(vec![note(Some("kept"), None, 99)]);
    let mut definition = FootnoteDefinition::new(Vec::new(), 99, Location::default());
    definition.replace_id(Some("kept"));
    edited.set_footnotes([definition]);
    edited.rebuild_footnotes()?;
    assert_eq!(catalog(&edited), [(Some("kept".to_owned()), 1)]);
    assert!(
        DocumentRef::from(&edited)
            .footnotes()
            .next()
            .ok_or("catalog")?
            .content()
            .is_empty()
    );
    Ok(())
}

#[rstest::rstest]
fn selected_content_and_derived_copies_are_not_registered_twice(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Doc\n:toc:\n\n== Heading footnote:first[First.]\n\n.Title footnote:second[Second.]\nimage::bird.png[]\n\nInline image:bird.png[alt='Alt footnote:third[Third.]'].\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.document().footnotes().count(), 3);
    let mut edited = Document::from(parsed.document());
    edited.rebuild_footnotes()?;
    assert_eq!(DocumentRef::from(&edited).footnotes().count(), 3);
    assert_eq!(
        DocumentRef::from(&edited)
            .footnotes()
            .map(FootnoteDefinitionRef::number)
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
    let before = serde_json::to_value(DocumentRef::from(&edited))?;
    edited.rebuild_footnotes()?;
    assert_eq!(serde_json::to_value(DocumentRef::from(&edited))?, before);
    Ok(())
}

#[rstest::rstest]
fn moved_blocks_refresh_toc_number_bindings_without_counting_copied_titles(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Doc\n:toc:\n\n== Heading footnote:heading[Heading note.]\n\nText.",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut edited = Document::from(parsed.document());
    let mut blocks: Vec<_> = edited.take_blocks().collect();
    blocks.insert(
        0,
        Block::Paragraph(Paragraph::new(
            vec![note(Some("prefix"), Some(vec![plain("Prefix.")]), 0)],
            Location::default(),
        )),
    );
    edited.set_blocks(blocks);
    edited.rebuild_footnotes()?;
    assert_eq!(
        catalog(&edited),
        [
            (Some("prefix".to_owned()), 1),
            (Some("heading".to_owned()), 2)
        ]
    );
    let toc = DocumentRef::from(&edited)
        .toc_entries()
        .next()
        .ok_or("toc entry")?;
    let marker = toc
        .title()
        .ok_or("title")?
        .inlines()
        .find_map(|inline| {
            if let InlineKind::Footnote(note) = inline.kind() {
                Some(note.number())
            } else {
                None
            }
        })
        .ok_or("toc marker")?;
    assert_eq!(marker, 2);
    Ok(())
}

#[rstest::rstest]
fn included_definition_provenance_survives_promotion_in_an_asciidoc_cell(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let included = directory.path().join("note.adoc");
    let main = directory.path().join("main.adoc");
    std::fs::write(
        &included,
        "unused\n  é footnote:included[Included *body*.]\n",
    )?;
    std::fs::write(
        &main,
        "[cols=a,options=noheader]\n|===\n|\ninclude::note.adoc[lines=2,indent=0]\n\nfootnote:included[] footnote:cell[Cell.]\n|===\n",
    )?;
    let parsed = acdc_parser::parse_file(
        &main,
        &Options::builder()
            .with_safe_mode(acdc_parser::SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let mut edited = Document::from(parsed.document());
    let cell = parsed
        .document()
        .descendants()
        .find_map(|node| {
            if let acdc_parser::ast::StructuralRef::TableCell(cell) = node {
                Some(cell)
            } else {
                None
            }
        })
        .ok_or("cell")?;
    let mut cell = acdc_parser::TableColumn::from(cell);
    let remaining = cell.take_blocks().skip(1);
    cell.set_blocks(remaining);
    edited.set_blocks([Block::DelimitedBlock(acdc_parser::DelimitedBlock::new(
        acdc_parser::DelimitedBlockType::DelimitedTable(acdc_parser::Table::new(
            vec![acdc_parser::TableRow::new(vec![cell])],
            Location::default(),
        )),
        "|===",
        Location::default(),
    ))]);
    edited.rebuild_footnotes()?;
    assert!(DocumentRef::from(&edited).footnotes().next().is_none());
    let cell = DocumentRef::from(&edited)
        .descendants()
        .find_map(|node| {
            if let acdc_parser::ast::StructuralRef::TableCell(cell) = node {
                Some(cell)
            } else {
                None
            }
        })
        .ok_or("cell")?;
    assert_eq!(
        cell.footnotes()
            .map(|note| (note.id(), note.number()))
            .collect::<Vec<_>>(),
        [(Some("included"), 1), (Some("cell"), 2)]
    );
    let definition = cell.footnotes().next().ok_or("included definition")?;
    assert_eq!(
        parsed.resolve_location(definition.location()).file,
        Some(included)
    );
    assert_eq!(definition.location().start().line(), 2);
    Ok(())
}

#[test]
fn replacing_a_catalog_body_does_not_override_a_retained_authored_body() -> Result<(), Error> {
    let mut edited = document(vec![note(Some("key"), Some(vec![plain("authored")]), 1)]);
    let mut definition = FootnoteDefinition::new(vec![plain("catalog")], 1, Location::default());
    definition.replace_id(Some("key"));
    edited.set_footnotes([definition]);
    edited.rebuild_footnotes()?;
    let definition = DocumentRef::from(&edited)
        .footnotes()
        .next()
        .ok_or("definition")?;
    let InlineKind::Plain(body) = definition.content().inlines().next().ok_or("body")?.kind()
    else {
        return Err("plain".into());
    };
    assert_eq!(body.text(), "authored");
    Ok(())
}

#[test]
fn duplicate_anonymous_catalog_numbers_are_ambiguous_and_preserve_the_old_catalog() {
    let mut edited = document(vec![note(None, None, 12)]);
    let definitions = [
        FootnoteDefinition::new(vec![plain("first")], 12, Location::default()),
        FootnoteDefinition::new(vec![plain("second")], 12, Location::default()),
    ];
    edited.set_footnotes(definitions.clone());
    assert!(matches!(
        edited.rebuild_footnotes(),
        Err(FootnoteRebuildError::AmbiguousDefinition { .. })
    ));
    assert_eq!(edited.take_footnotes().collect::<Vec<_>>(), definitions);
}

#[rstest::rstest]
#[case("compatibility_image_alt_snapshots.adoc")]
#[case("compatibility_caption_snapshots.adoc")]
#[case("compatibility_reference_snapshots.adoc")]
#[case("subs_verbatim_footnotes.adoc")]
#[case("xref_nested_footnotes.adoc")]
#[case("subs_index_control_snapshots.adoc")]
#[case("inline_index_contexts.adoc")]
#[case("title_macro_registration.adoc")]
#[case("subs_profile_replacement_order.adoc")]
#[case("compatibility_quote_content.adoc")]
fn rebuilding_source_trees_keeps_selected_note_membership(
    #[case] fixture: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/tests")
        .join(fixture);
    let parsed = acdc_parser::parse_file(
        path,
        &Options::builder()
            .with_safe_mode(acdc_parser::SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let mut original: Vec<_> = parsed
        .document()
        .footnotes()
        .map(|note| {
            (
                note.id().map(str::to_owned),
                note.location().absolute_start(),
                note.location().absolute_end(),
            )
        })
        .collect();
    let mut edited = Document::from(parsed.document());
    edited.rebuild_footnotes()?;
    let mut repaired: Vec<_> = DocumentRef::from(&edited)
        .footnotes()
        .map(|note| {
            (
                note.id().map(str::to_owned),
                note.location().absolute_start(),
                note.location().absolute_end(),
            )
        })
        .collect();
    original.sort();
    repaired.sort();
    assert_eq!(repaired, original, "{fixture} ({mode:?})");
    Ok(())
}
