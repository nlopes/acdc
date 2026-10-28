use std::{borrow::Cow, path::Path};

use acdc_parser::{
    AttributeValue, Block, ColumnFormat, ColumnStyle, ColumnWidth, CompatibilityMode,
    ContentAttribute, DelimitedBlock, DelimitedBlockType, ElementId, HorizontalAlignment, Image,
    InlineImage, InlineMacro, InlineNode, Location, Options, SafeMode, Source, SourceUrl, Table,
    TableFrame, TableGrid, TableRow, TableStripes, Title, VerticalAlignment,
    ast::{
        AttributeValueRef, BlockKind, BlockRef, BoldRef, ColumnFormatRef, ContentRef, ElementIdRef,
        ImageRef, InlineImageRef, InlineKind, InlineRef, SourceKind, SourceRef, TableCellRef,
        TableRef, TableRowRef,
    },
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

fn plain(content: ContentRef<'_>) -> Result<&str, Error> {
    let mut inlines = content.inlines();
    let Some(InlineKind::Plain(text)) = inlines.next().map(InlineRef::kind) else {
        return Err("expected plain text".into());
    };
    assert!(inlines.next().is_none());
    Ok(text.text())
}

fn bold_word(bold: BoldRef<'_>) -> Result<&str, Error> {
    plain(bold.content())
}

fn table(block: BlockRef<'_>) -> Result<TableRef<'_>, Error> {
    let BlockKind::Table(table) = block.kind() else {
        return Err("expected table".into());
    };
    Ok(table)
}

fn image_in(block: BlockRef<'_>) -> Result<InlineImageRef<'_>, Error> {
    let BlockKind::Paragraph(paragraph) = block.kind() else {
        return Err("expected paragraph".into());
    };
    let mut inlines = paragraph.content().inlines();
    let Some(InlineKind::Image(image)) = inlines.next().map(InlineRef::kind) else {
        return Err("expected inline image".into());
    };
    assert!(inlines.next().is_none());
    Ok(image)
}

fn row_text(row: TableRowRef<'_>) -> Result<Vec<&str>, Error> {
    row.cells().map(cell_text).collect()
}

fn cell_text(cell: TableCellRef<'_>) -> Result<&str, Error> {
    let Some(BlockKind::Paragraph(paragraph)) = cell.blocks().next().map(BlockRef::kind) else {
        return Err("expected cell paragraph".into());
    };
    plain(paragraph.content())
}

#[rstest::rstest]
fn nested_reads_keep_concrete_types_content_and_declarations(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = {
        let input = String::from(
            "= Read contract\n\n== Section\n\nBefore.\n\n\
             [cols=a,frame=none,grid=none]\n|===\n|\n\
             image:photo.png[alt='*Bird*',title='*Hover*',width=120]\n|===\n\nAfter.\n",
        );
        parse(
            &input,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?
    };
    let Some(BlockKind::Section(section)) =
        parsed.document_ref().blocks().next().map(BlockRef::kind)
    else {
        return Err("expected section".into());
    };
    let mut children = section.blocks();
    assert!(matches!(
        children.next().map(BlockRef::kind),
        Some(BlockKind::Paragraph(_))
    ));
    let table = table(children.next().ok_or("missing table")?)?;
    assert!(matches!(
        children.next().map(BlockRef::kind),
        Some(BlockKind::Paragraph(_))
    ));
    assert!(children.next().is_none());
    let row = table.rows().next().ok_or("missing row")?;
    let cell = row.cells().next().ok_or("missing cell")?;
    let image = image_in(cell.blocks().next().ok_or("missing cell body")?)?;
    assert_eq!(
        image.source().kind(),
        SourceKind::Path(Path::new("photo.png"))
    );
    let Some(InlineKind::Bold(alt)) = image
        .alt()
        .ok_or("missing alt")?
        .inlines()
        .next()
        .map(InlineRef::kind)
    else {
        return Err("expected bold alt".into());
    };
    let Some(InlineKind::Bold(tooltip)) = image
        .tooltip()
        .ok_or("missing tooltip")?
        .inlines()
        .next()
        .map(InlineRef::kind)
    else {
        return Err("expected bold tooltip".into());
    };
    assert_eq!(bold_word(alt)?, "Bird");
    assert_eq!(bold_word(tooltip)?, "Hover");
    assert_eq!(
        table.declared_frame(),
        Some(AttributeValueRef::Text("none"))
    );
    assert_eq!(table.declared_grid(), Some(AttributeValueRef::Text("none")));
    assert_eq!(table.declared_stripes(), None);
    let presentation = table.presentation().ok_or("missing presentation")?;
    assert_eq!(presentation.frame(), TableFrame::None);
    assert_eq!(presentation.grid(), TableGrid::None);
    assert_eq!(alt.location().start().line(), 10);
    assert_eq!(tooltip.location().start().line(), 10);
    assert!(alt.location().start().column() < tooltip.location().start().column());
    assert!(parsed.resolve_location(alt.location()).file.is_none());
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[test]
fn rows_columns_and_spans_preserve_table_order() -> Result<(), Error> {
    let parsed = parse(
        "[cols=\"1,2\",options=\"header,footer\"]\n|===\n|Head one |Head two\n\n\
         |Body one |Body two\n|Foot one |Foot two\n|===\n\n\
         [cols=\">.>s,<.<e,^.^h\"]\n|===\n2.2+^.^e|Combined\n|First companion\n\
         |Second companion\n|First\n|Second\n|Third\n|===\n",
        &Options::default(),
    )?;
    let mut blocks = parsed.document_ref().blocks();
    let first = table(blocks.next().ok_or("missing first table")?)?;
    assert_eq!(
        row_text(first.header().ok_or("missing header")?)?,
        ["Head one", "Head two"]
    );
    assert_eq!(first.rows().count(), 1);
    assert_eq!(
        row_text(first.rows().next().ok_or("missing body")?)?,
        ["Body one", "Body two"]
    );
    assert_eq!(
        row_text(first.footer().ok_or("missing footer")?)?,
        ["Foot one", "Foot two"]
    );
    assert_eq!(
        first
            .columns()
            .map(ColumnFormatRef::width)
            .collect::<Vec<_>>(),
        [ColumnWidth::Proportional(1), ColumnWidth::Proportional(2)]
    );

    let second = table(blocks.next().ok_or("missing second table")?)?;
    assert!(second.header().is_none());
    assert!(second.footer().is_none());
    let column = second.columns().next().ok_or("missing column")?;
    assert_eq!(column.halign(), HorizontalAlignment::Right);
    assert_eq!(column.valign(), VerticalAlignment::Bottom);
    assert_eq!(column.style(), ColumnStyle::Strong);
    let mut rows = second.rows();
    let first_row = rows.next().ok_or("missing first row")?;
    assert_eq!(first_row.cells().count(), 2);
    let combined = first_row.cells().next().ok_or("missing spanning cell")?;
    assert_eq!((combined.colspan(), combined.rowspan()), (2, 2));
    assert_eq!(combined.halign(), Some(HorizontalAlignment::Center));
    assert_eq!(combined.valign(), Some(VerticalAlignment::Middle));
    assert_eq!(combined.style(), Some(ColumnStyle::Emphasis));
    let second_row = rows.next().ok_or("missing second row")?;
    assert_eq!(second_row.cells().count(), 1);
    assert_eq!(
        second_row
            .cells()
            .next()
            .ok_or("missing companion")?
            .style(),
        Some(ColumnStyle::Strong)
    );
    assert_eq!(rows.next().ok_or("missing third row")?.cells().count(), 3);
    assert!(rows.next().is_none());
    Ok(())
}

#[test]
fn local_declarations_remain_separate_from_source_order_presentation() -> Result<(), Error> {
    let parsed = parse(
        "= Tables\n:table-frame: sides\n:table-grid: rows\n:table-stripes: odd\n\n\
         |===\n|one\n|===\n\n:table-frame: ends\n:table-grid: cols\n\n\
         |===\n|two\n|===\n\n[frame=topbot,grid=INVALID,stripes=\"\"]\n|===\n|three\n|===\n",
        &Options::default(),
    )?;
    let mut tables = parsed
        .document_ref()
        .blocks()
        .filter_map(|block| table(block).ok());
    for (frame, grid) in [
        (TableFrame::Sides, TableGrid::Rows),
        (TableFrame::Ends, TableGrid::Columns),
    ] {
        let table = tables.next().ok_or("missing table")?;
        assert_eq!(table.declared_frame(), None);
        assert_eq!(table.declared_grid(), None);
        assert_eq!(table.declared_stripes(), None);
        let presentation = table.presentation().ok_or("missing presentation")?;
        assert_eq!(
            (
                presentation.frame(),
                presentation.grid(),
                presentation.stripes()
            ),
            (frame, grid, TableStripes::Odd)
        );
    }
    let table = tables.next().ok_or("missing local table")?;
    assert_eq!(
        table.declared_frame(),
        Some(AttributeValueRef::Text("topbot"))
    );
    assert_eq!(
        table.declared_grid(),
        Some(AttributeValueRef::Text("INVALID"))
    );
    assert_eq!(table.declared_stripes(), Some(AttributeValueRef::Text("")));
    let presentation = table.presentation().ok_or("missing presentation")?;
    assert_eq!(
        (
            presentation.frame(),
            presentation.grid(),
            presentation.stripes()
        ),
        (TableFrame::Ends, TableGrid::None, TableStripes::None)
    );
    Ok(())
}

#[test]
fn constructed_tables_preserve_presence_and_exact_scalar_values() -> Result<(), Error> {
    let owner = Table::new(Vec::new(), Location::default())
        .with_header(Some(TableRow::new(Vec::new())))
        .with_columns(vec![
            ColumnFormat::new().with_width(ColumnWidth::Percentage(75)),
        ]);
    let mut block = DelimitedBlock::new(
        DelimitedBlockType::DelimitedTable(owner),
        "|===",
        Location::default(),
    )
    .with_title(Title::new(Vec::new()));
    let text = String::from("\"literal quotes\"");
    let pointer = text.as_ptr();
    block
        .metadata
        .attributes
        .set("frame".into(), AttributeValue::String(Cow::Owned(text)));
    block.metadata.attributes.set("grid".into(), false.into());
    block
        .metadata
        .attributes
        .set("stripes".into(), AttributeValue::None);
    let table = table(BlockRef::from(&block))?;
    assert!(table.title().is_some_and(ContentRef::is_empty));
    assert_eq!(
        table
            .header()
            .ok_or("missing empty header")?
            .cells()
            .count(),
        0
    );
    assert!(table.footer().is_none());
    assert_eq!(table.rows().count(), 0);
    assert_eq!(
        table.columns().next().ok_or("missing column")?.width(),
        ColumnWidth::Percentage(75)
    );
    assert!(table.presentation().is_none());
    let Some(AttributeValueRef::Text(text)) = table.declared_frame() else {
        return Err("expected exact text".into());
    };
    assert_eq!(text, "\"literal quotes\"");
    assert_eq!(text.as_ptr(), pointer);
    assert_eq!(table.declared_grid(), Some(AttributeValueRef::Bool(false)));
    assert_eq!(table.declared_stripes(), Some(AttributeValueRef::None));
    Ok(())
}

#[rstest::rstest]
fn images_preserve_independent_content_and_identity(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        ".Caption\n[#block,alt='*Bird*']\nimage::bird.png[]\n\n\
         image:inline.png[alt=\"\",title=\"\",id=inline]\n\n\
         .Table title\n[#table]\n|===\n|Cell\n|===\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut blocks = parsed.document_ref().blocks();
    let Some(BlockKind::Image(image)) = blocks.next().map(BlockRef::kind) else {
        return Err("missing block image".into());
    };
    assert_eq!(plain(image.title().ok_or("missing caption")?)?, "Caption");
    let Some(InlineKind::Bold(alt)) = image
        .alt()
        .ok_or("missing alt")?
        .inlines()
        .next()
        .map(InlineRef::kind)
    else {
        return Err("expected bold alt".into());
    };
    assert_eq!(bold_word(alt)?, "Bird");
    assert_eq!(image.id().ok_or("missing block ID")?.as_str(), "block");
    assert_eq!(
        image
            .id()
            .ok_or("missing block ID")?
            .location()
            .start()
            .line(),
        2
    );
    assert_eq!(image.source().to_string(), "bird.png");
    let inline = image_in(blocks.next().ok_or("missing inline image")?)?;
    assert!(inline.alt().is_some_and(ContentRef::is_empty));
    assert!(inline.tooltip().is_some_and(ContentRef::is_empty));
    assert_eq!(inline.id().ok_or("missing inline ID")?.as_str(), "inline");
    let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.get(1) else {
        return Err("missing owner paragraph".into());
    };
    let Some(InlineNode::Macro(InlineMacro::Image(owner))) = paragraph.content.first() else {
        return Err("missing owner image".into());
    };
    assert_eq!(
        parsed.resolve_location(inline.id().ok_or("missing inline ID")?.location()),
        parsed.source_location(
            &owner
                .metadata
                .id
                .as_ref()
                .ok_or("missing owner ID")?
                .location
        )
    );
    let table = table(blocks.next().ok_or("missing titled table")?)?;
    assert_eq!(
        plain(table.title().ok_or("missing table title")?)?,
        "Table title"
    );
    assert_eq!(table.id().ok_or("missing table ID")?.as_str(), "table");
    assert_eq!(
        table
            .id()
            .ok_or("missing table ID")?
            .location()
            .start()
            .line(),
        8
    );

    let mut image = Image::new(Source::Name("bird"), Location::default());
    assert!(ImageRef::from(&image).alt().is_none());
    assert!(ImageRef::from(&image).title().is_none());
    image.alt = Some(ContentAttribute::default());
    assert!(
        ImageRef::from(&image)
            .alt()
            .is_some_and(ContentRef::is_empty)
    );
    let mut inline = InlineImage::new(Source::Name("bird"), Location::default());
    assert!(InlineImageRef::from(&inline).alt().is_none());
    assert!(InlineImageRef::from(&inline).tooltip().is_none());
    inline.tooltip = Some(ContentAttribute::default());
    inline.metadata.id = Some(ElementId::new("constructed", Location::default()));
    assert!(
        InlineImageRef::from(&inline)
            .tooltip()
            .is_some_and(ContentRef::is_empty)
    );
    assert_eq!(
        InlineImageRef::from(&inline)
            .id()
            .ok_or("missing constructed ID")?
            .as_str(),
        "constructed"
    );
    Ok(())
}

#[test]
fn source_reads_preserve_url_spelling_and_target_kind() -> Result<(), Error> {
    let spelling = "HTTPS://Example.COM:443/a%2fb?x=%2F";
    let url = Source::Url(SourceUrl::new(spelling)?);
    assert_eq!(SourceRef::from(&url).kind(), SourceKind::Url(spelling));
    assert_eq!(SourceRef::from(&url).to_string(), spelling);
    let name = Source::Name("video-name");
    assert_eq!(
        SourceRef::from(&name).kind(),
        SourceKind::Name("video-name")
    );
    let path = Source::Path("../image.png".into());
    assert_eq!(
        SourceRef::from(&path).kind(),
        SourceKind::Path(Path::new("../image.png"))
    );
    #[cfg(unix)]
    {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let path = std::path::PathBuf::from(OsString::from_vec(vec![b'a', 0xff]));
        let source = Source::Path(path.clone());
        assert_eq!(SourceRef::from(&source).kind(), SourceKind::Path(&path));
    }
    Ok(())
}

#[test]
fn included_table_image_content_resolves_to_original_coordinates() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let primary = directory.path().join("main.adoc");
    let included = directory.path().join("table.adoc");
    std::fs::write(&primary, "include::table.adoc[tag=selected]\n")?;
    std::fs::write(
        &included,
        "Ignored.\n// tag::selected[]\n[cols=a]\n|===\n|\nimage:bird.png[alt='*猫*']\n|===\n// end::selected[]\n",
    )?;
    let parsed = parse_file(
        &primary,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let table = table(
        parsed
            .document_ref()
            .blocks()
            .next()
            .ok_or("missing included table")?,
    )?;
    let cell = table
        .rows()
        .next()
        .ok_or("missing row")?
        .cells()
        .next()
        .ok_or("missing cell")?;
    let image = image_in(cell.blocks().next().ok_or("missing paragraph")?)?;
    let Some(InlineKind::Bold(bold)) = image
        .alt()
        .ok_or("missing alt")?
        .inlines()
        .next()
        .map(InlineRef::kind)
    else {
        return Err("expected bold alt".into());
    };
    assert_eq!(bold_word(bold)?, "猫");
    let location = parsed.resolve_location(bold.location());
    assert_eq!(location.file, Some(included));
    assert_eq!(location.location.start.line, 6);
    assert_eq!(location.location.start.column, 21);
    assert_eq!(location.location.end.column, 23);
    assert_eq!(
        bold.location()
            .start()
            .include_chain()
            .ok_or("missing include chain")?
            .last(),
        Some("table.adoc")
    );
    Ok(())
}

#[test]
fn reads_support_shared_iterators_and_leave_the_document_unchanged() -> Result<(), Error> {
    fn handle<T: Copy + Send + Sync>() {}
    fn iterator<I: Iterator + Send + Sync>(value: I) -> I {
        value
    }
    handle::<TableRef<'_>>();
    handle::<TableRowRef<'_>>();
    handle::<TableCellRef<'_>>();
    handle::<ColumnFormatRef<'_>>();
    handle::<ImageRef<'_>>();
    handle::<InlineImageRef<'_>>();
    handle::<SourceRef<'_>>();
    handle::<ElementIdRef<'_>>();
    handle::<AttributeValueRef<'_>>();
    let parsed = parse(
        "[#table]\n[cols=a]\n|===\n|\nimage:bird.png[alt='*Bird*']\n|===\n",
        &Options::default(),
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let diagnostics = format!("{:?}", parsed.warnings());
    let references = parsed.document().references.len();
    for _ in 0..2 {
        let table = table(
            parsed
                .document_ref()
                .blocks()
                .next()
                .ok_or("missing table")?,
        )?;
        let rows = iterator(table.rows());
        let columns = iterator(table.columns());
        std::thread::scope(|scope| {
            scope.spawn(move || assert_eq!(rows.count(), 1));
            scope.spawn(move || assert_eq!(columns.count(), 1));
        });
        let mut cells = iterator(table.rows().next().ok_or("missing row")?.cells());
        let mut blocks = iterator(cells.next().ok_or("missing cell")?.blocks());
        let image = image_in(blocks.next().ok_or("missing block")?)?;
        let mut inlines = iterator(image.alt().ok_or("missing alt")?.inlines());
        assert!(matches!(
            inlines.next().map(InlineRef::kind),
            Some(InlineKind::Bold(_))
        ));
        assert!(table.id().is_some());
    }
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    assert_eq!(format!("{:?}", parsed.warnings()), diagnostics);
    assert_eq!(parsed.document().references.len(), references);
    assert_eq!(parsed.document().footnotes, []);
    Ok(())
}
