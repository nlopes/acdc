use acdc_parser::{
    Block, CompatibilityMode, InlineMacro, InlineNode, Location, Options, Paragraph, SafeMode,
    Title,
    ast::{
        BlockKind, BlockRef, BoldRef, ContentRef, DocumentRef, InlineKind, InlineRef, LocationRef,
        ParagraphRef, PositionRef, SectionRef,
    },
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

fn plain(content: ContentRef<'_>) -> Result<&str, Error> {
    let mut inlines = content.inlines();
    let Some(inline) = inlines.next() else {
        return Err("missing inline".into());
    };
    let InlineKind::Plain(text) = inline.kind() else {
        return Err("expected plain text".into());
    };
    assert!(inlines.next().is_none());
    Ok(text.text())
}

fn bold_word(bold: BoldRef<'_>) -> Result<&str, Error> {
    plain(bold.content())
}

#[rstest::rstest]
fn read_sections_titles_and_bodies_after_the_input_is_dropped(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = {
        let input = String::from(
            "= Views\n\n== *One*\n\n.First paragraph\nHello, *world*.\n\n\
             === Child\n\nChild body.\n\n== Two\n\nSecond body.\n",
        );
        parse(
            &input,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?
    };
    assert_eq!(
        plain(
            parsed
                .document_ref()
                .title()
                .ok_or("missing document title")?
        )?,
        "Views"
    );
    let mut roots = parsed.document_ref().blocks();
    let Some(BlockKind::Section(first)) = roots.next().map(BlockRef::kind) else {
        return Err("missing first section".into());
    };
    assert_eq!(first.level(), 1);
    let title = first.title().ok_or("missing section title")?;
    let Some(InlineKind::Bold(bold)) = title.inlines().next().map(InlineRef::kind) else {
        return Err("expected bold title".into());
    };
    assert_eq!(bold_word(bold)?, "One");
    let mut children = first.blocks();
    let Some(BlockKind::Paragraph(paragraph)) = children.next().map(BlockRef::kind) else {
        return Err("missing paragraph".into());
    };
    assert_eq!(
        plain(paragraph.title().ok_or("missing paragraph title")?)?,
        "First paragraph"
    );
    let mut body = paragraph.content().inlines();
    let Some(InlineKind::Plain(text)) = body.next().map(InlineRef::kind) else {
        return Err("missing paragraph text".into());
    };
    assert_eq!(text.text(), "Hello, ");
    let Some(InlineKind::Bold(bold)) = body.next().map(InlineRef::kind) else {
        return Err("missing bold body text".into());
    };
    assert_eq!(bold_word(bold)?, "world");
    let Some(BlockKind::Section(child)) = children.next().map(BlockRef::kind) else {
        return Err("missing child section".into());
    };
    assert_eq!(child.level(), 2);
    assert_eq!(plain(child.title().ok_or("missing child title")?)?, "Child");
    assert!(children.next().is_none());
    let Some(BlockKind::Section(second)) = roots.next().map(BlockRef::kind) else {
        return Err("missing second section".into());
    };
    assert_eq!(plain(second.title().ok_or("missing second title")?)?, "Two");
    assert!(roots.next().is_none());
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[test]
fn constructed_titles_keep_presence_without_copying_the_owner() {
    let missing = Paragraph::new(Vec::new(), Location::default());
    assert!(ParagraphRef::from(&missing).title().is_none());
    let empty = missing.with_title(Title::new(Vec::new()));
    assert!(
        ParagraphRef::from(&empty)
            .title()
            .is_some_and(ContentRef::is_empty)
    );
    assert!(ParagraphRef::from(&empty).content().is_empty());
    assert!(empty.title.content().is_some_and(ContentRef::is_empty));
}

#[test]
fn borrowed_nodes_and_direct_iterators_support_shared_reads() -> Result<(), Error> {
    fn handle<T: Copy + Send + Sync>() {}
    fn iterator<I: Iterator + Send + Sync>(value: I) -> I {
        value
    }
    handle::<DocumentRef<'_>>();
    handle::<SectionRef<'_>>();
    handle::<ParagraphRef<'_>>();
    handle::<ContentRef<'_>>();
    let parsed = parse("A paragraph.", &Options::default())?;
    let blocks = iterator(parsed.document_ref().blocks());
    std::thread::scope(|scope| {
        scope.spawn(move || assert_eq!(blocks.count(), 1));
        assert_eq!(parsed.document_ref().blocks().count(), 1);
    });
    let Some(BlockKind::Paragraph(paragraph)) =
        parsed.document_ref().blocks().next().map(BlockRef::kind)
    else {
        return Err("missing paragraph".into());
    };
    let inlines = iterator(paragraph.content().inlines());
    std::thread::scope(|scope| {
        scope.spawn(move || assert_eq!(inlines.count(), 1));
    });
    Ok(())
}

#[test]
fn semantic_kinds_flatten_tables_and_macros() -> Result<(), Error> {
    let parsed = parse(
        "|===\n|Cell\n|===\n\nimage::block.png[Block]\n\nimage:inline.png[Inline]\n",
        &Options::default(),
    )?;
    let mut blocks = parsed.document_ref().blocks();
    let table = blocks.next().ok_or("missing table")?;
    let BlockKind::Table(table_view) = table.kind() else {
        return Err("expected table kind".into());
    };
    assert_eq!(table_view.location().start().line(), 1);
    assert_eq!(table.location().end().line(), 3);
    assert!(matches!(
        blocks.next().map(BlockRef::kind),
        Some(BlockKind::Image(_))
    ));
    let Some(BlockKind::Paragraph(paragraph)) = blocks.next().map(BlockRef::kind) else {
        return Err("missing inline-image paragraph".into());
    };
    let image = paragraph
        .content()
        .inlines()
        .next()
        .ok_or("missing image")?;
    assert!(matches!(image.kind(), InlineKind::Image(_)));
    assert_eq!(image.location().start().line(), 7);
    assert!(blocks.next().is_none());
    Ok(())
}

#[test]
fn locations_retain_unicode_coordinates() -> Result<(), Error> {
    let parsed = parse("é *猫*", &Options::default())?;
    let Some(BlockKind::Paragraph(paragraph)) =
        parsed.document_ref().blocks().next().map(BlockRef::kind)
    else {
        return Err("missing paragraph".into());
    };
    let Some(InlineKind::Bold(bold)) = paragraph.content().inlines().nth(1).map(InlineRef::kind)
    else {
        return Err("missing bold span".into());
    };
    assert_eq!(bold.location().start().column(), 3);
    assert_eq!(bold.location().end().column(), 5);
    assert_eq!(bold.location().absolute_start(), 3);
    assert_eq!(bold.location().absolute_end(), 7);
    assert!(bold.location().start().include_chain().is_none());
    assert_eq!(
        parsed
            .resolve_location(bold.location())
            .location
            .start
            .column,
        3
    );
    Ok(())
}

#[test]
fn included_nodes_resolve_to_the_original_file() -> Result<(), Error> {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/preprocessor");
    let primary = directory.join("include_tag_diagnostics_main.adoc");
    let parsed = parse_file(
        &primary,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let selected = parsed
        .document_ref()
        .blocks()
        .find_map(|block| {
            let BlockKind::Paragraph(paragraph) = block.kind() else {
                return None;
            };
            (plain(paragraph.content()).ok() == Some("Selected.")).then_some(paragraph)
        })
        .ok_or("missing selected paragraph")?;
    let source = parsed.resolve_location(selected.location());
    assert_eq!(
        source.file,
        Some(directory.join("include_tag_diagnostics_target.adoc"))
    );
    assert_eq!(source.location.start.line, 2);
    assert_eq!(
        selected
            .location()
            .start()
            .include_chain()
            .ok_or("missing include chain")?
            .last(),
        Some("include_tag_diagnostics_target.adoc")
    );
    let first = parsed
        .document_ref()
        .blocks()
        .next()
        .ok_or("missing primary block")?;
    assert_eq!(
        parsed.resolve_location(first.location()).file,
        Some(primary)
    );
    Ok(())
}

#[test]
fn reads_preserve_json_catalogs_and_diagnostics() -> Result<(), Error> {
    let parsed = parse(
        "= Views\n:toc:\n\n[[one]]\n== One\n\nText footnote:[Note] and <<one>>.\n",
        &Options::default(),
    )?;
    let json = serde_json::to_value(parsed.document())?;
    let warnings = format!("{:?}", parsed.warnings());
    let footnotes = parsed.document().footnotes.len();
    let references = parsed.document().references.len();
    let toc_entries = parsed.document().toc_entries.len();
    assert_eq!(footnotes, 1);
    assert!(references > 0);
    assert_eq!(toc_entries, 1);
    for _ in 0..2 {
        for block in parsed.document_ref().blocks() {
            if let BlockKind::Section(section) = block.kind() {
                assert_eq!(plain(section.title().ok_or("missing title")?)?, "One");
                for child in section.blocks() {
                    if let BlockKind::Paragraph(paragraph) = child.kind() {
                        assert!(
                            paragraph
                                .content()
                                .inlines()
                                .any(|inline| matches!(inline.kind(), InlineKind::Footnote(_)))
                        );
                    }
                }
            }
        }
    }
    assert_eq!(serde_json::to_value(parsed.document())?, json);
    assert_eq!(format!("{:?}", parsed.warnings()), warnings);
    assert_eq!(parsed.document().footnotes.len(), footnotes);
    assert_eq!(parsed.document().references.len(), references);
    assert_eq!(parsed.document().toc_entries.len(), toc_entries);
    Ok(())
}

#[test]
fn owner_conversions_preserve_typed_content_and_locations() -> Result<(), Error> {
    let parsed = parse(
        "== Section\n\nimage:bird.png[Bird]\n\n|===\n|Cell\n|===\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    assert_eq!(DocumentRef::from(document).blocks().count(), 1);
    let block = document.blocks.first().ok_or("missing section")?;
    assert!(matches!(
        BlockRef::from(block).kind(),
        BlockKind::Section(_)
    ));
    let Block::Section(section) = block else {
        return Err("expected section".into());
    };
    assert_eq!(SectionRef::from(section).level(), 1);
    let Some(Block::Paragraph(paragraph)) = section.content.first() else {
        return Err("missing paragraph".into());
    };
    assert_eq!(ParagraphRef::from(paragraph).content().inlines().count(), 1);
    let inline = paragraph.content.first().ok_or("missing inline image")?;
    assert!(matches!(
        InlineRef::from(inline).kind(),
        InlineKind::Image(_)
    ));
    let InlineNode::Macro(inline_macro) = inline else {
        return Err("expected macro".into());
    };
    assert!(matches!(
        InlineRef::from(inline_macro).kind(),
        InlineKind::Image(_)
    ));
    let InlineMacro::Image(image) = inline_macro else {
        return Err("expected image".into());
    };
    assert_eq!(
        plain(ContentRef::from(image.alt.as_ref().ok_or("missing alt")?))?,
        "Bird"
    );
    let Some(Block::DelimitedBlock(table)) = section.content.get(1) else {
        return Err("missing table".into());
    };
    assert!(matches!(BlockRef::from(table).kind(), BlockKind::Table(_)));
    assert_eq!(LocationRef::from(&paragraph.location).start().line(), 3);
    assert_eq!(PositionRef::from(&paragraph.location.start).column(), 1);
    Ok(())
}

#[test]
fn copied_source_span_keeps_provenance_after_its_owner_is_dropped() {
    let mut expected = Location::default();
    expected.absolute_start = 41;
    expected.absolute_end = 73;
    expected.start.line = 7;
    expected.start.column = 3;
    expected.start.file = Some(std::sync::Arc::new(vec![
        "part.adoc".into(),
        "nested/é.adoc".into(),
    ]));
    expected.end.line = 8;
    expected.end.column = 5;
    expected.end.file = expected.start.file.clone();
    let copied = {
        let owner = expected.clone();
        Location::from(LocationRef::from(&owner))
    };
    assert_eq!(copied, expected);
}
