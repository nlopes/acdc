//! Prepared body edits preserve source snapshots and explicit content presence.

use acdc_parser::{
    Admonition, AdmonitionVariant, Block, BlockMetadata, CompatibilityMode, DelimitedBlock,
    DelimitedBlockType, Location, Options, Paragraph, StemContent, StemNotation, Table, TableFrame,
    Title,
    ast::{AdmonitionRef, BlockKind, BlockRef, ContentRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn owned_block(parsed: &acdc_parser::ParseResult) -> Result<Block<'_>, Error> {
    parsed
        .document()
        .blocks()
        .next()
        .map(Block::from)
        .ok_or_else(|| "block".into())
}

fn table_frame(block: &DelimitedBlock<'_>) -> Result<TableFrame, Error> {
    let BlockKind::Table(table) = BlockRef::from(block).kind() else {
        return Err("table".into());
    };
    Ok(table.presentation().ok_or("table presentation")?.frame())
}

#[test]
fn every_delimited_body_round_trips_without_a_placeholder_api() -> Result<(), Error> {
    for body in [
        DelimitedBlockType::DelimitedComment(Vec::new()),
        DelimitedBlockType::DelimitedExample(Vec::new()),
        DelimitedBlockType::DelimitedListing(Vec::new()),
        DelimitedBlockType::DelimitedLiteral(Vec::new()),
        DelimitedBlockType::DelimitedOpen(Vec::new()),
        DelimitedBlockType::DelimitedSidebar(Vec::new()),
        DelimitedBlockType::DelimitedTable(Table::new(Vec::new(), Location::default())),
        DelimitedBlockType::DelimitedPass(Vec::new()),
        DelimitedBlockType::DelimitedQuote(Vec::new()),
        DelimitedBlockType::DelimitedVerse(Vec::new()),
        DelimitedBlockType::DelimitedStem(StemContent::new("λ + x", StemNotation::Latexmath)),
    ] {
        let mut block = DelimitedBlock::new(body, "----", Location::default());
        block.set_title(Title::new(Vec::new()));
        assert_eq!(block.clone_body(), block.clone().into_body());
        let before = serde_json::to_value(&block)?;
        let expected = block.clone();
        let previous = block.replace_body(DelimitedBlockType::DelimitedOpen(Vec::new()));
        block.replace_body(previous);
        let metadata = block.take_metadata();
        let title = block.take_title();
        block.set_metadata(metadata)?;
        block.set_title(title);
        assert_eq!(block, expected);
        assert_eq!(serde_json::to_value(&block)?, before);
        assert_eq!(block.source_text(), None);
    }
    Ok(())
}

#[rstest::rstest]
fn parsed_body_transfers_keep_source_spelling_locations_and_prepared_content(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    for source in [
        "[#listing]\n.Title\n[source,rust]\n----\nλ <1>\n----\n",
        ".Quote\n[quote,'*Author*','_Work_']\n____\nText footnote:[Note].\n____\n",
        ".Table\n|===\n|Cell\n|===\n",
        "[latexmath]\n++++\nλ + x\n++++\n",
    ] {
        let parsed = parse(
            source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let document_before = serde_json::to_value(parsed.document())?;
        let Block::DelimitedBlock(mut block) = owned_block(&parsed)? else {
            return Err("delimited".into());
        };
        let original = block.clone();
        let expected = serde_json::to_value(&block)?;
        let snapshot = block.source_text();
        let body = block.replace_body(DelimitedBlockType::DelimitedOpen(Vec::new()));
        let metadata = block.take_metadata();
        let title = block.take_title();
        block.replace_body(body);
        block.set_metadata(metadata)?;
        block.set_title(title);
        assert_eq!(block, original);
        assert_eq!(serde_json::to_value(&block)?, expected);
        assert_eq!(block.source_text(), snapshot);
        assert_eq!(serde_json::to_value(parsed.document())?, document_before);
    }
    Ok(())
}

#[rstest::rstest]
fn transferred_body_parts_outlive_the_old_owner(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[#example]\n.A *title*\n====\nNested *body*.\n====",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Block::DelimitedBlock(expected) = owned_block(&parsed)? else {
        return Err("delimited".into());
    };
    let (body, metadata, title) = {
        let Block::DelimitedBlock(mut local) = owned_block(&parsed)? else {
            return Err("delimited".into());
        };
        let result = (
            local.replace_body(DelimitedBlockType::DelimitedOpen(Vec::new())),
            local.take_metadata(),
            local.take_title(),
        );
        drop(local);
        result
    };
    let mut rebuilt = DelimitedBlock::new(
        body,
        "====",
        Location::from(BlockRef::from(&expected).location()),
    );
    rebuilt.set_metadata(metadata)?;
    rebuilt.set_title(title);
    assert_eq!(
        serde_json::to_value(&rebuilt)?,
        serde_json::to_value(&expected)?
    );
    assert_eq!(rebuilt.source_text(), None);
    Ok(())
}

#[rstest::rstest]
fn table_body_replacement_and_metadata_removal_refresh_source_position_defaults(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        ":table-frame: sides\n\n|===\n|First\n|===\n\n:table-frame: ends\n\n|===\n|Second\n|===",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut tables = parsed
        .document()
        .blocks()
        .filter(|block| matches!(block.kind(), BlockKind::Table(_)))
        .map(Block::from);
    let Some(Block::DelimitedBlock(mut first)) = tables.next() else {
        return Err("first table".into());
    };
    let Some(Block::DelimitedBlock(mut second)) = tables.next() else {
        return Err("second table".into());
    };
    let mut metadata = first.clone_metadata();
    metadata.attributes.set("frame".into(), "none".into());
    first.set_metadata(metadata)?;
    let incoming = second.replace_body(DelimitedBlockType::DelimitedOpen(Vec::new()));
    first.replace_body(incoming);
    assert_eq!(table_frame(&first)?, TableFrame::None);
    let metadata = first.take_metadata();
    assert_eq!(table_frame(&first)?, TableFrame::Ends);
    first.set_metadata(metadata)?;
    assert_eq!(table_frame(&first)?, TableFrame::None);
    let before = first.clone();
    let mut invalid = first.clone_metadata();
    invalid.attributes.set("title".into(), "shadow".into());
    assert!(first.set_metadata(invalid).is_err());
    assert_eq!(first, before);
    assert_eq!(table_frame(&first)?, TableFrame::None);
    Ok(())
}

#[rstest::rstest]
fn simple_and_compound_admonitions_keep_form_and_empty_title_on_round_trip(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    for source in [
        ".Title\nNOTE: Body footnote:[Note].",
        ".Title\n[NOTE]\n====\nBody footnote:[Note].\n====",
    ] {
        let parsed = parse(
            source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let Block::Admonition(mut admonition) = owned_block(&parsed)? else {
            return Err("admonition".into());
        };
        let before = serde_json::to_value(&admonition)?;
        let simple = AdmonitionRef::from(&admonition)
            .simple_paragraph()
            .is_some();
        let blocks = admonition.take_blocks();
        let metadata = admonition.take_metadata();
        let title = admonition.take_title();
        assert_eq!(AdmonitionRef::from(&admonition).blocks().count(), 0);
        assert!(AdmonitionRef::from(&admonition).title().is_none());
        admonition.set_blocks(blocks);
        admonition.set_metadata(metadata)?;
        admonition.set_title(title);
        assert_eq!(serde_json::to_value(&admonition)?, before);
        assert_eq!(
            AdmonitionRef::from(&admonition)
                .simple_paragraph()
                .is_some(),
            simple
        );
        admonition.set_title(Title::new(Vec::new()));
        assert!(
            AdmonitionRef::from(&admonition)
                .title()
                .is_some_and(ContentRef::is_empty)
        );
        let _ = admonition.take_title();
        assert!(AdmonitionRef::from(&admonition).title().is_none());
    }
    Ok(())
}

#[test]
fn admonition_owned_children_preserve_lifetimes_and_metadata_failures_are_atomic()
-> Result<(), Error> {
    let text = String::from("Borrowed body");
    let body = Paragraph::new(
        vec![acdc_parser::InlineNode::PlainText(acdc_parser::Plain::new(
            &text,
            Location::default(),
        ))],
        Location::default(),
    );
    let mut original =
        Admonition::from_paragraph(AdmonitionVariant::Note, body, Location::default());
    let children = original.take_blocks();
    drop(original);
    let mut rebuilt = Admonition::new(AdmonitionVariant::Warning, Vec::new(), Location::default());
    rebuilt.set_blocks(children);
    rebuilt.push_block(Block::Paragraph(Paragraph::new(
        Vec::new(),
        Location::default(),
    )));
    rebuilt.set_variant(AdmonitionVariant::Tip);
    rebuilt.set_location(Location::default());
    assert_eq!(
        AdmonitionRef::from(&rebuilt).variant(),
        AdmonitionVariant::Tip
    );
    assert_eq!(AdmonitionRef::from(&rebuilt).blocks().count(), 2);
    assert!(serde_json::to_string(&rebuilt)?.contains("Borrowed body"));
    for name in [
        "title",
        "id",
        "roles",
        "opts",
        "caption",
        "subs",
        "attribution",
        "citetitle",
    ] {
        let before = rebuilt.clone();
        let mut invalid = rebuilt.clone_metadata();
        invalid.attributes.set(name.into(), "shadow".into());
        assert!(rebuilt.set_metadata(invalid).is_err(), "{name}");
        assert_eq!(rebuilt, before, "{name}");
    }
    let mut metadata = BlockMetadata::new();
    metadata.attributes.set("alt".into(), "extension".into());
    rebuilt.set_metadata(metadata)?;
    assert!(AdmonitionRef::from(&rebuilt).attributes().contains("alt"));
    Ok(())
}

#[rstest::rstest]
fn span_and_delimiter_edits_do_not_reparse_or_relocate_body(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "----\nλ *literal*\n----",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Block::DelimitedBlock(mut block) = owned_block(&parsed)? else {
        return Err("listing".into());
    };
    let BlockKind::Listing(before) = BlockRef::from(&block).kind() else {
        return Err("listing".into());
    };
    let content = serde_json::to_value(&block)?
        .get("inlines")
        .ok_or("inlines")?
        .clone();
    let source = block.source_text();
    let body_span = Location::from(
        before
            .content()
            .inlines()
            .next()
            .ok_or("inline")?
            .location(),
    );
    block.set_location(Location::default());
    block.set_delimiter("....");
    block.set_delimiter_locations(None, None);
    let BlockKind::Listing(after) = BlockRef::from(&block).kind() else {
        return Err("body reparsed".into());
    };
    assert_eq!(serde_json::to_value(&block)?.get("inlines"), Some(&content));
    assert_eq!(after.source_text(), source);
    assert_eq!(after.delimiter(), "....");
    assert!(after.open_delimiter_location().is_none());
    assert!(after.close_delimiter_location().is_none());
    assert_eq!(Location::from(after.location()), Location::default());
    assert_eq!(
        Location::from(after.content().inlines().next().ok_or("inline")?.location()),
        body_span
    );
    Ok(())
}
