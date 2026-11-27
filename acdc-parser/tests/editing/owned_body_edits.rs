//! Prepared body edits preserve source snapshots and explicit content presence.

use acdc_parser::{
    AdmonitionVariant, CompatibilityMode, Location, Options, StemNotation, TableFrame,
    ast::{AdmonitionRef, BlockKind, ContentRef, ExampleRef, ListingRef, TableRef},
    edit::{
        Admonition, Block, BlockMetadata, DelimitedComment, Example, Listing, Literal, Open,
        Paragraph, Passthrough, Quote, Sidebar, StemBlock, Table, Title, Verse,
    },
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

fn table_frame(table: &Table<'_>) -> Result<TableFrame, Error> {
    Ok(TableRef::from(table)
        .presentation()
        .ok_or("table presentation")?
        .frame())
}

/// Move a delimited block's body out and back in, then do the same for its
/// metadata and title, returning the block for comparison.
macro_rules! transfer_parts {
    ($block:expr, |$owner:ident| $body:block) => {{
        let mut $owner = $block;
        $body
        let metadata = $owner.take_metadata();
        let title = $owner.take_title();
        $owner.set_metadata(metadata)?;
        $owner.set_title(title);
        $owner
    }};
}

/// Move a compound body out and back in.
macro_rules! transfer_blocks {
    ($block:expr) => {
        transfer_parts!($block, |block| {
            let body: Vec<_> = block.take_blocks().collect();
            block.set_blocks(body);
        })
    };
}

/// Move an inline body out and back in.
macro_rules! transfer_content {
    ($block:expr) => {
        transfer_parts!($block, |block| {
            let body: Vec<_> = block.take_content().collect();
            block.set_content(body);
        })
    };
}

/// Move every table body part out and back in.
macro_rules! transfer_table {
    ($block:expr) => {
        transfer_parts!($block, |block| {
            let header = block.take_header();
            let footer = block.take_footer();
            let rows: Vec<_> = block.take_rows().collect();
            let columns: Vec<_> = block.take_columns().collect();
            block.set_header(header);
            block.set_footer(footer);
            block.set_rows(rows);
            block.set_columns(columns);
        })
    };
}

/// Move a formula out and back in.
macro_rules! transfer_formula {
    ($block:expr) => {
        transfer_parts!($block, |block| {
            let text = block.replace_text("");
            let notation = block.replace_notation(StemNotation::Asciimath);
            block.replace_text(text);
            block.replace_notation(notation);
        })
    };
}

#[test]
fn every_delimited_body_round_trips_without_a_placeholder_api() -> Result<(), Error> {
    macro_rules! round_trip {
        ($block:expr, $transfer:ident) => {{
            let mut block = $block;
            block.set_delimiter("----");
            block.set_title(Title::new(Vec::new()));
            assert_eq!($transfer!(block.clone()), block);
            let before = serde_json::to_value(&block)?;
            let expected = block.clone();
            let block = $transfer!(block);
            assert_eq!(block, expected);
            assert_eq!(serde_json::to_value(&block)?, before);
            assert_eq!(block.source_text(), None);
        }};
    }
    round_trip!(
        DelimitedComment::new(Vec::new(), Location::default()),
        transfer_content
    );
    round_trip!(
        Example::new(Vec::new(), Location::default()),
        transfer_blocks
    );
    round_trip!(
        Listing::new(Vec::new(), Location::default()),
        transfer_content
    );
    round_trip!(
        Literal::new(Vec::new(), Location::default()),
        transfer_content
    );
    round_trip!(Open::new(Vec::new(), Location::default()), transfer_blocks);
    round_trip!(
        Sidebar::new(Vec::new(), Location::default()),
        transfer_blocks
    );
    round_trip!(Table::new(Vec::new(), Location::default()), transfer_table);
    round_trip!(
        Passthrough::new(Vec::new(), Location::default()),
        transfer_content
    );
    round_trip!(Quote::new(Vec::new(), Location::default()), transfer_blocks);
    round_trip!(
        Verse::new(Vec::new(), Location::default()),
        transfer_content
    );
    round_trip!(
        StemBlock::new("λ + x", StemNotation::Latexmath, Location::default()),
        transfer_formula
    );
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
        let original = owned_block(&parsed)?;
        let expected = serde_json::to_value(&original)?;
        let copy = original.clone();
        let (block, snapshot, transferred_snapshot) = if let Block::DelimitedListing(block) = copy {
            let snapshot = block.source_text();
            let block = transfer_content!(block);
            let transferred = block.source_text();
            (Block::from(block), snapshot, transferred)
        } else if let Block::DelimitedQuote(block) = copy {
            let snapshot = block.source_text();
            let block = transfer_blocks!(block);
            let transferred = block.source_text();
            (Block::from(block), snapshot, transferred)
        } else if let Block::DelimitedTable(block) = copy {
            let snapshot = block.source_text();
            let block = transfer_table!(block);
            let transferred = block.source_text();
            (Block::from(block), snapshot, transferred)
        } else if let Block::DelimitedStem(block) = copy {
            let snapshot = block.source_text();
            let block = transfer_formula!(block);
            let transferred = block.source_text();
            (Block::from(block), snapshot, transferred)
        } else {
            return Err("delimited".into());
        };
        assert_eq!(block, original);
        assert_eq!(serde_json::to_value(&block)?, expected);
        assert_eq!(transferred_snapshot, snapshot);
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
    let Block::DelimitedExample(expected) = owned_block(&parsed)? else {
        return Err("example".into());
    };
    let (body, metadata, title) = {
        let Block::DelimitedExample(mut local) = owned_block(&parsed)? else {
            return Err("example".into());
        };
        let result = (
            local.take_blocks().collect::<Vec<_>>(),
            local.take_metadata(),
            local.take_title(),
        );
        drop(local);
        result
    };
    let mut rebuilt = Example::new(body, Location::from(ExampleRef::from(&expected).location()));
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
        .filter(|block| matches!(block.kind(), BlockKind::DelimitedTable(_)))
        .map(Block::from);
    let Some(Block::DelimitedTable(first)) = tables.next() else {
        return Err("first table".into());
    };
    let Some(Block::DelimitedTable(second)) = tables.next() else {
        return Err("second table".into());
    };
    let mut metadata = first.clone_metadata();
    metadata.attributes.set("frame".into(), "none".into());
    // A table body carries the defaults of its source position, so the
    // second table's body takes the first table's metadata.
    let mut first = second;
    first.set_metadata(metadata)?;
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
        vec![acdc_parser::edit::InlineNode::Plain(
            acdc_parser::edit::Plain::new(&text, Location::default()),
        )],
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
    let Block::DelimitedListing(mut block) = owned_block(&parsed)? else {
        return Err("listing".into());
    };
    let before = ListingRef::from(&block);
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
    let after = ListingRef::from(&block);
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
