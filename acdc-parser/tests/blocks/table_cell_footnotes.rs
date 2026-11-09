use acdc_parser::{
    CompatibilityMode, Location, Options, WarningKind,
    ast::{DocumentRef, FootnoteDefinitionRef, StructuralRef, TableCellRef},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

fn cells(document: DocumentRef<'_>) -> Vec<TableCellRef<'_>> {
    document
        .descendants()
        .filter_map(|node| {
            if let StructuralRef::TableCell(cell) = node {
                Some(cell)
            } else {
                None
            }
        })
        .collect()
}

fn catalog<'a>(
    definitions: impl Iterator<Item = FootnoteDefinitionRef<'a>>,
) -> Vec<(Option<&'a str>, u32)> {
    definitions.map(|note| (note.id(), note.number())).collect()
}

fn catalog_values(doc: DocumentRef<'_>) -> Vec<Vec<serde_json::Value>> {
    cells(doc).into_iter().map(|cell| cell.footnotes().map(|note| serde_json::json!({"id": note.id(), "number": note.number(), "content": acdc_parser::ContentAttribute::from(note.content()), "location": Location::from(note.location())})).collect::<Vec<_>>()).collect::<Vec<_>>()
}

fn table(values: &[&str]) -> String {
    let mut source = String::from("[cols=a,options=noheader]\n|===\n");
    for value in values {
        source.push_str("|\n");
        source.push_str(value);
        source.push_str("\n\n");
    }
    source.push_str("|===\n");
    source
}

#[rstest::rstest]
fn source_names_stay_in_their_owning_document(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!(
        "= Notes\n\nfootnote:same[Outer.]\n\n{}\nfootnote:cell[] footnote:same[]\n",
        table(&[
            "footnote:same[] footnote:cell[First.] footnote:cell[]",
            "footnote:cell[] footnote:cell[Second.] footnote:cell[]",
        ])
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(catalog(parsed.document().footnotes()), [(Some("same"), 1)]);
    let owners = cells(parsed.document());
    assert_eq!(owners.len(), 2);
    for (cell, number) in owners.iter().zip([2, 3]) {
        assert!(cell.owns_footnotes());
        assert_eq!(catalog(cell.footnotes()), [(Some("cell"), number)]);
    }
    let invalid = parsed.warnings().iter().filter(|warning| matches!(&warning.kind,
        WarningKind::ContentRecovery { message } if message.starts_with("invalid footnote reference:"))).count();
    assert_eq!(invalid, 3);
    let native = serde_json::to_value(parsed.document())?;
    assert_eq!(
        native.pointer("/blocks/1/content/rows/0/columns/0/footnote_scope"),
        Some(&serde_json::json!(true))
    );
    let json = serde_json::to_string(&native)?;
    assert_eq!(json.matches("First.").count(), 1);
    assert_eq!(json.matches("Second.").count(), 1);
    Ok(())
}

#[rstest::rstest]
fn nested_owners_and_ordinary_cells_keep_global_numbers(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let nested = "footnote:parent[Parent.]\n\n[cols=a,options=noheader]\n!===\n!\nfootnote:parent[] footnote:child[Child.]\n!===\n\nfootnote:child[] footnote:parent[]";
    let source = format!(
        "= Notes\n\n{}\nfootnote:parent[] footnote:outer[Outer.]\n\n[cols=\"1,1\",options=noheader]\n|===\n|footnote:outer[] |footnote:normal[Normal.]\n|===\n\nfootnote:normal[]\n",
        table(&[nested])
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(
        catalog(parsed.document().footnotes()),
        [(Some("outer"), 3), (Some("normal"), 4)]
    );
    let owners = cells(parsed.document());
    assert_eq!(owners.len(), 4);
    assert_eq!(
        catalog(owners.first().ok_or("parent")?.footnotes()),
        [(Some("parent"), 1)]
    );
    assert_eq!(
        catalog(owners.get(1).ok_or("child")?.footnotes()),
        [(Some("child"), 2)]
    );
    assert!(
        owners
            .iter()
            .skip(2)
            .all(|cell| !cell.owns_footnotes() && cell.footnotes().next().is_none())
    );
    Ok(())
}

#[rstest::rstest]
fn reference_labels_register_under_their_owning_cell(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!(
        "= Notes\n\n{}\nfootnote:late[]\n",
        table(&[
            "[#target,reftext='Label footnote:late[Late.]']\n.Title footnote:title[Title.]\n====\nBody footnote:body[Body.].\n====",
            "footnote:late[]",
        ])
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(catalog(parsed.document().footnotes()), []);
    let owners = cells(parsed.document());
    let expected = if mode == CompatibilityMode::Draft {
        vec![(Some("title"), 1), (Some("late"), 2), (Some("body"), 3)]
    } else {
        vec![(Some("title"), 1), (Some("body"), 2)]
    };
    assert_eq!(catalog(owners.first().ok_or("cell")?.footnotes()), expected);
    assert_eq!(catalog(owners.get(1).ok_or("sibling")?.footnotes()), []);
    Ok(())
}

#[rstest::rstest]
fn selected_includes_keep_cell_provenance(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    std::fs::write(
        directory.path().join("notes.adoc"),
        "Excluded footnote:wrong[Wrong.]\nfootnote:included[Included.]\nfootnote:included[]\n",
    )?;
    let source = directory.path().join("main.adoc");
    std::fs::write(
        &source,
        format!(
            "= Notes\n\n{}\nfootnote:included[]\n",
            table(&["include::notes.adoc[lines=2..3]"])
        ),
    )?;
    let parsed = parse_file(
        &source,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(acdc_parser::SafeMode::Unsafe)
            .build()?,
    )?;
    assert_eq!(catalog(parsed.document().footnotes()), []);
    let owners = cells(parsed.document());
    let note = owners
        .first()
        .ok_or("cell")?
        .footnotes()
        .next()
        .ok_or("note")?;
    assert_eq!((note.id(), note.number()), (Some("included"), 1));
    let location = Location::from(note.location());
    assert_eq!(location.start.line, 2);
    assert_eq!(
        location.start.file.as_deref().map(Vec::as_slice),
        Some(["notes.adoc".to_owned()].as_slice())
    );
    Ok(())
}

fn cell_table(cells: Vec<acdc_parser::TableCell<'_>>) -> acdc_parser::Block<'_> {
    use acdc_parser::{Block, DelimitedBlock, DelimitedBlockType, Table, TableRow};
    Block::DelimitedBlock(DelimitedBlock::new(
        DelimitedBlockType::DelimitedTable(Table::new(
            vec![TableRow::new(cells)],
            Location::default(),
        )),
        "|===",
        Location::default(),
    ))
}

#[rstest::rstest]
fn prepared_cell_repair_uses_local_catalogs_and_one_number_sequence(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    use acdc_parser::{Document, TableCell};
    let source = format!(
        "= Notes\n\n{}",
        table(&[
            "footnote:same[First.]\n\nfootnote:same[]",
            "footnote:same[Second.]\n\nfootnote:same[]",
        ])
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut owners = cells(parsed.document())
        .into_iter()
        .map(TableCell::from)
        .collect::<Vec<_>>();
    for cell in &mut owners {
        let remaining = cell.take_blocks().skip(1);
        cell.set_blocks(remaining);
    }
    owners.reverse();
    let mut edited = Document::from(parsed.document());
    edited.set_blocks([cell_table(owners)]);
    edited.rebuild_footnotes()?;
    assert_eq!(catalog(DocumentRef::from(&edited).footnotes()), []);
    let owners = cells(DocumentRef::from(&edited));
    for (owner, number, body) in owners.iter().zip([1, 2]).map(|(owner, number)| {
        (
            owner,
            number,
            if number == 1 { "Second." } else { "First." },
        )
    }) {
        assert_eq!(catalog(owner.footnotes()), [(Some("same"), number)]);
        let definition = owner.footnotes().next().ok_or("definition")?;
        let content =
            serde_json::to_string(&acdc_parser::ContentAttribute::from(definition.content()))?;
        assert!(content.contains(body));
    }
    let before = serde_json::to_value(DocumentRef::from(&edited))?;
    edited.rebuild_footnotes()?;
    assert_eq!(serde_json::to_value(DocumentRef::from(&edited))?, before);
    Ok(())
}

#[test]
fn failed_cell_repair_cannot_borrow_a_sibling_definition() -> Result<(), Error> {
    use acdc_parser::{Document, FootnoteRebuildError, TableCell};
    let source = format!(
        "= Notes\n\n{}",
        table(&[
            "footnote:same[First.]",
            "footnote:same[Second.]\n\nfootnote:same[]"
        ])
    );
    let parsed = parse(&source, &Options::default())?;
    let mut owners = cells(parsed.document())
        .into_iter()
        .map(TableCell::from)
        .collect::<Vec<_>>();
    let second = owners.get_mut(1).ok_or("second")?;
    let remaining = second.take_blocks().skip(1);
    second.set_blocks(remaining);
    second.set_footnotes(Some(Vec::new()));
    let mut edited = Document::from(parsed.document());
    edited.set_blocks([cell_table(owners)]);
    let before = serde_json::to_value(DocumentRef::from(&edited))?;
    let retained = catalog_values(DocumentRef::from(&edited));
    assert!(
        matches!(edited.rebuild_footnotes(), Err(FootnoteRebuildError::MissingDefinition { id: Some(id), .. }) if id == "same")
    );
    assert_eq!(serde_json::to_value(DocumentRef::from(&edited))?, before);
    assert_eq!(catalog_values(DocumentRef::from(&edited)), retained);
    Ok(())
}

#[rstest::rstest]
fn derived_reference_titles_keep_the_source_owners_number_binding(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    use acdc_parser::{Document, TableCell, ast::InlineKind};
    let source = format!(
        "= Notes\n\nfootnote:same[Outer.]\n\n{}",
        table(&[
            "[#first]\n.Title footnote:same[First.]\n====\nBody.\n====",
            "[#second]\n.Title footnote:same[Second.]\n====\nBody.\n====",
        ])
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut owners = cells(parsed.document())
        .into_iter()
        .map(TableCell::from)
        .collect::<Vec<_>>();
    owners.reverse();
    let mut edited = Document::from(parsed.document());
    let outer = edited.take_blocks().next().ok_or("outer")?;
    edited.set_blocks([cell_table(owners), outer]);
    edited.rebuild_footnotes()?;
    let read = DocumentRef::from(&edited);
    assert_eq!(catalog(read.footnotes()), [(Some("same"), 3)]);
    for (target, expected) in [("first", 2), ("second", 1)] {
        let title = read
            .references()
            .get(target)
            .ok_or("target")?
            .title()
            .ok_or("title")?;
        let number = title.inlines().find_map(|node| {
            if let InlineKind::Footnote(note) = node.kind() {
                Some(note.number())
            } else {
                None
            }
        });
        assert_eq!(number, Some(expected));
    }
    Ok(())
}

#[test]
fn nested_document_labels_rebind_each_footnote_once() -> Result<(), Error> {
    use acdc_parser::{
        Block, ContentAttribute, CrossReference, Document, Footnote, InlineMacro, InlineNode,
        Paragraph, ast::InlineKind,
    };
    let source = format!(
        "= Notes\n\nfootnote:same[Outer.]\n\n{}",
        table(&["footnote:same[Cell.]"])
    );
    let parsed = parse(&source, &Options::default())?;
    let mut inner = CrossReference::new("", Location::default());
    inner.set_document_reftext(Some(ContentAttribute::new(vec![InlineNode::Macro(
        InlineMacro::Footnote(Footnote::reference(Some("same"), 1, Location::default())),
    )])));
    let mut outer = CrossReference::new("", Location::default());
    outer.set_document_reftext(Some(ContentAttribute::new(vec![InlineNode::Macro(
        InlineMacro::CrossReference(Box::new(inner)),
    )])));
    let mut edited = Document::from(parsed.document());
    let mut blocks = edited.take_blocks();
    let outer_note = blocks.next().ok_or("outer note")?;
    let cell = blocks.next().ok_or("cell table")?;
    edited.set_blocks([
        cell,
        outer_note,
        Block::Paragraph(Paragraph::new(
            vec![
                InlineNode::Macro(InlineMacro::CrossReference(Box::new(outer.clone()))),
                InlineNode::Macro(InlineMacro::CrossReference(Box::new(outer))),
            ],
            Location::default(),
        )),
    ]);
    for _ in 0..2 {
        edited.rebuild_footnotes()?;
        assert_eq!(
            catalog(DocumentRef::from(&edited).footnotes()),
            [(Some("same"), 2)]
        );
        let acdc_parser::ast::BlockKind::Paragraph(labels) = DocumentRef::from(&edited)
            .blocks()
            .nth(2)
            .ok_or("labels")?
            .kind()
        else {
            return Err("paragraph".into());
        };
        for label in labels.content().inlines() {
            let InlineKind::CrossReference(outer) = label.kind() else {
                return Err("outer xref".into());
            };
            let InlineKind::CrossReference(inner) = outer
                .document_reftext()
                .ok_or("outer label")?
                .inlines()
                .next()
                .ok_or("inner xref")?
                .kind()
            else {
                return Err("inner xref".into());
            };
            let InlineKind::Footnote(note) = inner
                .document_reftext()
                .ok_or("inner label")?
                .inlines()
                .next()
                .ok_or("note")?
                .kind()
            else {
                return Err("note".into());
            };
            assert_eq!(note.number(), 2);
        }
    }
    Ok(())
}
