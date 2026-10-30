use std::ops::ControlFlow;

use acdc_parser::{
    Block, CompatibilityMode, DelimitedBlock, DelimitedBlockType, Document, Location, Options,
    Paragraph, SafeMode, Table, TableRow,
    ast::{BlockKind, BlockRef, ContentRef, DocumentRef, InlineKind, StructuralRef},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn document_assignments_are_structural_only_when_retained_as_blocks(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Title\n:header-entry: one\n\nBefore.\n\n:body-entry: two\n\nAfter.\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let document = parsed.document_ref();
    assert_eq!(document.descendants().count(), 4);
    let names = document
        .descendants()
        .filter_map(|node| {
            if let StructuralRef::Block(block) = node {
                if let BlockKind::DocumentAttribute(assignment) = block.kind() {
                    return Some(assignment.name());
                }
            }
            None
        })
        .collect::<Vec<_>>();
    assert_eq!(names, ["body-entry"]);
    assert!(document.attributes().contains("header-entry"));
    Ok(())
}

fn text(content: ContentRef<'_>) -> String {
    content
        .inlines()
        .filter_map(|inline| {
            if let InlineKind::Plain(text) = inline.kind() {
                Some(text.text())
            } else {
                None
            }
        })
        .collect()
}

fn label(node: StructuralRef<'_>) -> String {
    if let StructuralRef::Header(_) = node {
        "header".into()
    } else if let StructuralRef::ListItem(item) = node {
        format!("item:{}", text(item.principal()))
    } else if let StructuralRef::DescriptionListItem(item) = node {
        format!("term:{}", text(item.term()))
    } else if let StructuralRef::CalloutListItem(item) = node {
        format!("callout:{}", text(item.principal()))
    } else if let StructuralRef::TableRow(_) = node {
        "row".into()
    } else if let StructuralRef::TableCell(_) = node {
        "cell".into()
    } else if let StructuralRef::Block(block) = node {
        if let BlockKind::Paragraph(paragraph) = block.kind() {
            text(paragraph.content())
        } else if matches!(block.kind(), BlockKind::Section(_)) {
            "section".into()
        } else if matches!(block.kind(), BlockKind::Example(_)) {
            "example".into()
        } else if matches!(block.kind(), BlockKind::Open(_)) {
            "open".into()
        } else if matches!(block.kind(), BlockKind::Sidebar(_)) {
            "sidebar".into()
        } else if matches!(block.kind(), BlockKind::Quote(_)) {
            "quote".into()
        } else if matches!(block.kind(), BlockKind::Admonition(_)) {
            "admonition".into()
        } else if matches!(block.kind(), BlockKind::UnorderedList(_)) {
            "ulist".into()
        } else if matches!(block.kind(), BlockKind::OrderedList(_)) {
            "olist".into()
        } else if matches!(block.kind(), BlockKind::DescriptionList(_)) {
            "dlist".into()
        } else if matches!(block.kind(), BlockKind::CalloutList(_)) {
            "colist".into()
        } else if matches!(block.kind(), BlockKind::Listing(_)) {
            "listing".into()
        } else if matches!(block.kind(), BlockKind::Table(_)) {
            "table".into()
        } else {
            "other block".into()
        }
    } else {
        "other structure".into()
    }
}

#[rstest::rstest]
fn compound_blocks_walk_in_preorder_without_inline_or_catalog_visits(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Title footnote:[Header note.]\n\n== Section\n\nParent.\n\n====\nExample.\n\n--\nOpen.\n--\n\n****\nSidebar.\n****\n\n____\nQuote.\n____\n\nNOTE: Note.\n====\n\nTail.\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let document = parsed.document_ref();
    assert_eq!(
        document.children().map(label).collect::<Vec<_>>(),
        ["header", "section"]
    );
    let expected = [
        "header",
        "section",
        "Parent.",
        "example",
        "Example.",
        "open",
        "Open.",
        "sidebar",
        "Sidebar.",
        "quote",
        "Quote.",
        "admonition",
        "Note.",
        "Tail.",
    ];
    assert_eq!(
        document.descendants().map(label).collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        StructuralRef::from(document)
            .descendants()
            .map(label)
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(document.footnotes().count(), 1);
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[rstest::rstest]
#[case("* First\n+\nAttached.\n+\n** Nested\n* Second\n", vec!["ulist", "item:First", "Attached.", "ulist", "item:Nested", "item:Second"])]
#[case(". First\n+\nAttached.\n. Second\n", vec!["olist", "item:First", "Attached.", "item:Second"])]
#[case("First:: Principal\n+\nAttached.\n\nSecond:: Other\n", vec!["dlist", "term:First", "Attached.", "term:Second"])]
#[case("----\ncode <1>\n----\n<1> Explanation\n+\nAttached.\n", vec!["listing", "colist", "callout:Explanation", "Attached."])]
fn lists_visit_items_before_their_attached_blocks(
    #[case] source: &str,
    #[case] expected: Vec<&str>,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    assert_eq!(
        parsed
            .document_ref()
            .descendants()
            .map(label)
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[rstest::rstest]
fn table_rows_cells_and_spans_are_visited_once_in_header_body_footer_order(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "[cols=\"a,a\",options=\"header,footer\"]\n|===\n|Head A |Head B\n2+|Body\n|Foot A |Foot B\n|===\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let table = parsed.document_ref().blocks().next().ok_or("table")?;
    assert_eq!(
        table.children().map(label).collect::<Vec<_>>(),
        ["row", "row", "row"]
    );
    assert_eq!(
        table.descendants().map(label).collect::<Vec<_>>(),
        [
            "row", "cell", "Head A", "cell", "Head B", "row", "cell", "Body", "row", "cell",
            "Foot A", "cell", "Foot B"
        ]
    );
    let spans = table
        .descendants()
        .filter_map(|node| {
            if let StructuralRef::TableCell(cell) = node {
                Some(cell.colspan())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(spans, [1, 1, 2, 1, 1]);
    Ok(())
}

#[rstest::rstest]
#[case("Plain *bold* footnote:[Note.]")]
#[case("image::bird.svg[Alt]")]
#[case("audio::sound.ogg[]")]
#[case("video::movie.mp4[]")]
#[case("toc::[]")]
#[case("[discrete]\n== Heading")]
#[case("<<<\n\nAfter.")]
#[case("'''\n")]
#[case("// Comment\n")]
#[case("////\nComment\n////")]
#[case("----\nCode.\n----")]
#[case("....\nLiteral.\n....")]
#[case("++++\nRaw.\n++++")]
#[case("[verse]\n____\nLine.\n____")]
#[case("[stem]\n++++\nx\n++++")]
fn structural_leaves_have_no_children(
    #[case] source: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let block = parsed.document_ref().blocks().next().ok_or("leaf block")?;
    assert!(block.children().next().is_none());
    let mut descendants = block.descendants();
    assert!(descendants.next().is_none());
    assert!(descendants.next().is_none());
    Ok(())
}

#[test]
fn empty_structures_still_keep_rows_and_early_breaks_can_resume() -> Result<(), Error> {
    let document = Document::default();
    assert!(DocumentRef::from(&document).descendants().next().is_none());
    let table = Table::new(vec![TableRow::new(Vec::new())], Location::default())
        .with_header(Some(TableRow::new(Vec::new())))
        .with_footer(Some(TableRow::new(Vec::new())));
    let owner = DelimitedBlock::new(
        DelimitedBlockType::DelimitedTable(table),
        "|===",
        Location::default(),
    );
    assert_eq!(
        BlockRef::from(&owner)
            .descendants()
            .map(label)
            .collect::<Vec<_>>(),
        ["row", "row", "row"]
    );

    let parsed = parse("====\nNested.\n====\n\nAfter.", &Options::default())?;
    let mut nodes = parsed.document_ref().descendants();
    let stopped = nodes.try_for_each(|node| {
        if label(node) == "example" {
            ControlFlow::Break(node)
        } else {
            ControlFlow::Continue(())
        }
    });
    assert!(stopped.is_break());
    assert_eq!(
        nodes.by_ref().map(label).collect::<Vec<_>>(),
        ["Nested.", "After."]
    );
    assert!(nodes.next().is_none());
    assert!(nodes.next().is_none());
    Ok(())
}

#[rstest::rstest]
fn included_and_cell_blocks_keep_original_source_locations(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let main = directory.path().join("main.adoc");
    let include = directory.path().join("body.adoc");
    std::fs::write(
        &main,
        "include::body.adoc[]\n\n[cols=a]\n|===\n|é Cell.\n|===\n",
    )?;
    std::fs::write(&include, "====\né Included.\n====\n")?;
    let parsed = parse_file(
        &main,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let nodes = {
        let document = parsed.document_ref();
        document.descendants()
    };
    let paragraphs = nodes
        .filter_map(|node| {
            if let StructuralRef::Block(block) = node {
                if let BlockKind::Paragraph(paragraph) = block.kind() {
                    return Some(paragraph);
                }
            }
            None
        })
        .collect::<Vec<_>>();
    let [included, cell] = paragraphs.as_slice() else {
        return Err("two paragraphs".into());
    };
    assert_eq!(
        parsed.resolve_location(included.location()).file,
        Some(include)
    );
    assert_eq!(parsed.resolve_location(cell.location()).file, Some(main));
    assert_eq!(text(included.content()), "é Included.");
    assert_eq!(text(cell.content()), "é Cell.");
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[test]
fn deep_traversal_uses_a_heap_stack_and_can_cross_threads() -> Result<(), Error> {
    fn shared<T: Send + Sync>(value: T) -> T {
        value
    }
    let mut tree = Block::Paragraph(Paragraph::new(Vec::new(), Location::default()));
    for _ in 0..2048 {
        tree = Block::DelimitedBlock(DelimitedBlock::new(
            DelimitedBlockType::DelimitedOpen(vec![
                tree,
                Block::Paragraph(Paragraph::new(Vec::new(), Location::default())),
            ]),
            "--",
            Location::default(),
        ));
    }
    let result: Result<usize, Error> = std::thread::scope(|scope| {
        let nodes = shared(BlockRef::from(&tree).descendants());
        std::thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn_scoped(scope, move || nodes.count())?
            .join()
            .map_err(|_| "traversal thread panicked".into())
    });
    // Drop the constructed tree iteratively too, so the test measures traversal stack use.
    while let Block::DelimitedBlock(block) = tree {
        let DelimitedBlockType::DelimitedOpen(mut blocks) = block.inner else {
            return Err("open block".into());
        };
        if blocks.is_empty() {
            return Err("nested block".into());
        }
        tree = blocks.remove(0);
    }
    let count: usize = result?;
    assert_eq!(count, 4096);
    Ok(())
}
