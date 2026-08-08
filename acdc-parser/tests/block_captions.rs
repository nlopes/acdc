//! Captions resolved by the parser: which blocks take one, from which attributes, and in what
//! order they are numbered.
//!
//! `BlockMetadata::caption` carries `#[serde(skip)]`, so the AST fixtures cannot cover any of
//! this — these go through the public API instead.

use std::num::NonZeroU32;

use acdc_parser::{
    AttributeValue, Block, BlockMetadata, Caption, CaptionKind, DelimitedBlockType,
    DelimitedBlockType::DelimitedExample, Document, InlineNode, Location, Options, Plain, Title,
    parse,
};

type Error = Box<dyn std::error::Error>;

fn caption<'block, 'a>(block: &'block Block<'a>) -> Option<&'block Caption<'a>> {
    if let Block::Paragraph(paragraph) = block {
        return paragraph.metadata.caption.as_ref();
    }
    if let Block::DelimitedBlock(delimited) = block {
        return delimited.metadata.caption.as_ref();
    }
    if let Block::Image(image) = block {
        return image.metadata.caption.as_ref();
    }
    None
}

/// The caption of a captioned block, as `(kind, label, ordinal)`.
fn numbered(block: &Block<'_>) -> Option<(CaptionKind, String, Option<u32>)> {
    if let Caption::Numbered {
        kind,
        label,
        number,
    } = caption(block)?
    {
        return Some((*kind, label.to_string(), number.map(NonZeroU32::get)));
    }
    None
}

#[test]
fn classification_follows_the_effective_block_context() -> Result<(), Error> {
    // A delimiter that carries a caption context of its own keeps it; a style only promotes the
    // verbatim delimiters and the open block, which carry none. Verified against `asciidoctor`.
    let cases: [(&str, Option<CaptionKind>); 14] = [
        ("====\nc\n====", Some(CaptionKind::Example)),
        ("[listing]\n====\nc\n====", Some(CaptionKind::Example)),
        ("[source,rust]\n====\nc\n====", Some(CaptionKind::Example)),
        ("----\nc\n----", Some(CaptionKind::Listing)),
        ("[example]\n----\nc\n----", Some(CaptionKind::Listing)),
        ("[literal]\n----\nc\n----", None),
        ("....\nc\n....", None),
        ("[listing]\n....\nc\n....", Some(CaptionKind::Listing)),
        ("[source,rust]\n....\nc\n....", Some(CaptionKind::Listing)),
        ("[example]\n....\nc\n....", None),
        ("[example]\n--\nc\n--", Some(CaptionKind::Example)),
        ("[listing]\n--\nc\n--", Some(CaptionKind::Listing)),
        ("--\nc\n--", None),
        ("[example]\n****\nc\n****", None),
    ];

    for (body, expected) in cases {
        let source = format!("= T\n:listing-caption: Listing\n\n.Title\n{body}\n");
        let parsed = parse(&source, &Options::default())?;
        let document = parsed.document();
        let block = document.blocks.first().ok_or("expected a block")?;
        let kind = numbered(block).map(|(kind, _, _)| kind);
        assert_eq!(kind, expected, "source: {body:?}");
    }
    Ok(())
}

#[test]
fn styled_paragraphs_take_the_caption_their_style_names() -> Result<(), Error> {
    let cases: [(&str, Option<CaptionKind>); 5] = [
        ("[example]\ntext", Some(CaptionKind::Example)),
        ("[listing]\ntext", Some(CaptionKind::Listing)),
        ("[source,rust]\ntext", Some(CaptionKind::Listing)),
        ("[literal]\ntext", None),
        ("text", None),
    ];

    for (body, expected) in cases {
        let source = format!("= T\n:listing-caption: Listing\n\n.Title\n{body}\n");
        let parsed = parse(&source, &Options::default())?;
        let document = parsed.document();
        let block = document.blocks.first().ok_or("expected a block")?;
        assert_eq!(
            numbered(block).map(|(kind, _, _)| kind),
            expected,
            "source: {body:?}"
        );
    }
    Ok(())
}

#[test]
fn nested_examples_are_numbered_inner_first() -> Result<(), Error> {
    // Asciidoctor parses a block's content before captioning the block, so an inner example
    // takes the lower ordinal.
    let parsed = parse(
        "= T\n\n.Outer\n====\n.Inner\n[example]\nInner body.\n====\n\n.After\n====\nx\n====\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    let [outer, after] = document.blocks.as_slice() else {
        return Err(format!("expected two blocks, got {:?}", document.blocks).into());
    };

    let Block::DelimitedBlock(delimited) = outer else {
        return Err("expected a delimited example".into());
    };
    let DelimitedExample(inner_blocks) = &delimited.inner else {
        return Err("expected example content".into());
    };
    let inner = inner_blocks.first().ok_or("expected an inner block")?;

    assert_eq!(numbered(inner).and_then(|(_, _, n)| n), Some(1));
    assert_eq!(numbered(outer).and_then(|(_, _, n)| n), Some(2));
    assert_eq!(numbered(after).and_then(|(_, _, n)| n), Some(3));
    Ok(())
}

#[test]
fn collapsible_examples_do_not_consume_example_numbers() -> Result<(), Error> {
    let parsed = parse(
        "= T\n\n.Before\n====\none\n====\n\n.Collapsible block\n[%collapsible,caption=\"Ignored: \"]\n====\ntwo\n====\n\n.Collapsible paragraph\n[example%collapsible]\nthree\n\n.Collapsible open block\n[example%collapsible]\n--\nfour\n--\n\n.After\n====\nfive\n====\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    let [before, block, paragraph, open, after] = document.blocks.as_slice() else {
        return Err(format!("expected five blocks, got {:?}", document.blocks).into());
    };

    assert_eq!(numbered(before).and_then(|(_, _, n)| n), Some(1));
    for collapsible in [block, paragraph, open] {
        assert_eq!(caption(collapsible), Some(&Caption::Custom("".into())));
    }
    assert_eq!(numbered(after).and_then(|(_, _, n)| n), Some(2));
    Ok(())
}

#[test]
fn a_list_continuation_block_takes_a_caption() -> Result<(), Error> {
    // Continuation blocks come from their own grammar rule, not the one plain blocks use.
    let parsed = parse(
        "= T\n\n* Item\n+\n.Continued\n====\nContent.\n====\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    let Some(Block::UnorderedList(list)) = document.blocks.first() else {
        return Err(format!("expected a list, got {:?}", document.blocks).into());
    };
    let block = list
        .items
        .first()
        .and_then(|item| item.blocks.first())
        .ok_or("expected a continuation block")?;

    assert_eq!(
        numbered(block),
        Some((CaptionKind::Example, "Example".to_string(), Some(1)))
    );
    Ok(())
}

#[test]
fn table_cells_are_numbered_header_then_rows_then_footer() -> Result<(), Error> {
    let parsed = parse(
        "= T\n\n[cols=\"1a\",options=\"header,footer\"]\n|===\n|.In header\n[example]\none\n\n|.In body\n[example]\ntwo\n\n|.In footer\n[example]\nthree\n|===\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    let Some(Block::DelimitedBlock(delimited)) = document.blocks.first() else {
        return Err(format!("expected a table, got {:?}", document.blocks).into());
    };
    let DelimitedBlockType::DelimitedTable(table) = &delimited.inner else {
        return Err("expected table content".into());
    };

    let mut seen = Vec::new();
    for row in table
        .header
        .iter()
        .chain(table.rows.iter())
        .chain(table.footer.iter())
    {
        for column in &row.columns {
            for block in &column.content {
                if let Some((_, _, Some(number))) = numbered(block) {
                    seen.push(number);
                }
            }
        }
    }
    // Source order: the footer sits last in the source even though it renders above the body.
    assert_eq!(seen, vec![1, 2, 3]);
    Ok(())
}

#[test]
fn an_attribute_inside_a_block_changes_that_blocks_own_caption() -> Result<(), Error> {
    // A block is captioned once its content is parsed, so an entry inside it applies to it.
    let parsed = parse(
        "= T\n:example-caption: Outer\n\n.Inner attr\n====\n:example-caption: Inner\ncontent\n====\n\n.After\n====\nmore\n====\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    let [first, second] = document.blocks.as_slice() else {
        return Err(format!("expected two blocks, got {:?}", document.blocks).into());
    };

    assert_eq!(
        numbered(first),
        Some((CaptionKind::Example, "Inner".to_string(), Some(1)))
    );
    assert_eq!(
        numbered(second),
        Some((CaptionKind::Example, "Inner".to_string(), Some(2)))
    );
    Ok(())
}

#[test]
fn blocks_keep_the_caption_active_at_their_source_position() -> Result<(), Error> {
    let parsed = parse(
        "= T\n:example-caption: Header\n\n.First\n[example]\none\n\n:example-caption!:\n\n.Second\n[example]\ntwo\n\n:example-caption: Later\n\n.Third\n[example]\nthree\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    assert_eq!(
        document.attributes.get_string("example-caption").as_deref(),
        Some("Later")
    );

    let captions: Vec<_> = document
        .blocks
        .iter()
        .filter(|block| matches!(block, Block::Paragraph(_)))
        .map(|block| caption(block).cloned())
        .collect();
    assert_eq!(
        captions,
        vec![
            Some(Caption::Numbered {
                kind: CaptionKind::Example,
                label: "Header".into(),
                number: NonZeroU32::new(1),
            }),
            // An unset label takes no caption and consumes no ordinal.
            Some(Caption::Unnumbered),
            Some(Caption::Numbered {
                kind: CaptionKind::Example,
                label: "Later".into(),
                number: NonZeroU32::new(2),
            }),
        ]
    );
    Ok(())
}

#[test]
fn a_metadata_line_applies_to_its_own_block() -> Result<(), Error> {
    // An attribute entry between a title and its block never becomes a `DocumentAttribute`
    // block, but it still applies to the block it sits on.
    let parsed = parse(
        "= T\n:example-caption: Header\n\n.First\n:example-caption: Metadata\n[example]\none\n\n.Second\n[example]\ntwo\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    assert!(
        !document
            .blocks
            .iter()
            .any(|block| matches!(block, Block::DocumentAttribute(_))),
        "a metadata-region entry produces no block: {:?}",
        document.blocks
    );

    let labels: Vec<_> = document
        .blocks
        .iter()
        .filter_map(|block| numbered(block).map(|(_, label, number)| (label, number)))
        .collect();
    assert_eq!(
        labels,
        vec![
            ("Metadata".to_string(), Some(1)),
            ("Metadata".to_string(), Some(2)),
        ]
    );
    Ok(())
}

#[test]
fn untitled_caption_capable_blocks_consume_no_ordinal() -> Result<(), Error> {
    let parsed = parse(
        "= T\n\n[example]\nuntitled\n\n.Titled\n[example]\ntitled\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    let [untitled, titled] = document.blocks.as_slice() else {
        return Err(format!("expected two blocks, got {:?}", document.blocks).into());
    };

    // The label still comes from the untitled block's own source position, so a consumer that
    // adds a title later does not lose it.
    assert_eq!(
        numbered(untitled),
        Some((CaptionKind::Example, "Example".to_string(), None))
    );
    assert_eq!(numbered(titled).and_then(|(_, _, n)| n), Some(1));
    Ok(())
}

#[test]
fn a_listing_takes_no_caption_until_its_label_is_set() -> Result<(), Error> {
    // Unlike `example-caption`, `listing-caption` has no default.
    let parsed = parse(
        "= T\n\n.Untitled label\n[listing]\nc\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    let block = document.blocks.first().ok_or("expected a block")?;
    assert_eq!(caption(block), Some(&Caption::Unnumbered));

    let parsed = parse(
        "= T\n:listing-caption: Listing\n\n.Set label\n[listing]\nc\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    let block = document.blocks.first().ok_or("expected a block")?;
    assert_eq!(
        numbered(block),
        Some((CaptionKind::Listing, "Listing".to_string(), Some(1)))
    );
    Ok(())
}

#[test]
fn caption_precedence_covers_element_generic_and_type_specific() -> Result<(), Error> {
    let parsed = parse(
        "= Captions\n:listing-caption: Listing\n\n.Type specific\n[listing]\none\n\n:caption: Generic: \n\n.Generic\n[listing]\ntwo\n\n.Element\n[listing,caption=Element: ]\nthree\n\n:caption:\n\n.Blank generic\n[listing]\nfour\n\n:caption!:\n\n.Fallback\n[listing]\nfive\n\n:listing-caption!:\n\n.Unnumbered\n[source,rust]\nsix\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    let captions: Vec<_> = document
        .blocks
        .iter()
        .filter(|block| matches!(block, Block::Paragraph(_)))
        .map(|block| caption(block).cloned())
        .collect();

    assert_eq!(
        captions,
        vec![
            Some(Caption::Numbered {
                kind: CaptionKind::Listing,
                label: "Listing".into(),
                number: NonZeroU32::new(1),
            }),
            // A generic `caption` wins over the type-specific label and takes no ordinal.
            Some(Caption::Custom("Generic:".into())),
            // A block's own `caption=` wins over both.
            Some(Caption::Custom("Element:".into())),
            Some(Caption::Custom("".into())),
            // Unsetting the generic one falls back to the type-specific label, which resumes
            // numbering where it left off.
            Some(Caption::Numbered {
                kind: CaptionKind::Listing,
                label: "Listing".into(),
                number: NonZeroU32::new(2),
            }),
            Some(Caption::Unnumbered),
        ]
    );
    Ok(())
}

#[test]
fn figures_and_tables_count_separately() -> Result<(), Error> {
    let parsed = parse(
        "= T\n\n.A figure\nimage::one.png[]\n\n.An example\n====\nc\n====\n\n.A table\n|===\n|cell\n|===\n\n.Another figure\nimage::two.png[]\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    let captions: Vec<_> = document.blocks.iter().filter_map(numbered).collect();

    assert_eq!(
        captions,
        vec![
            (CaptionKind::Figure, "Figure".to_string(), Some(1)),
            (CaptionKind::Example, "Example".to_string(), Some(1)),
            (CaptionKind::Table, "Table".to_string(), Some(1)),
            (CaptionKind::Figure, "Figure".to_string(), Some(2)),
        ]
    );
    Ok(())
}

#[test]
fn renumber_captions_restores_numbering_after_a_mutation() -> Result<(), Error> {
    fn ordinals(document: &Document<'_>) -> Vec<Option<u32>> {
        document
            .blocks
            .iter()
            .filter_map(|block| numbered(block).map(|(_, _, number)| number))
            .collect()
    }

    let parsed = parse(
        "= T\n\n.One\n====\na\n====\n\n.Two\n====\nb\n====\n\n.Three\n====\nc\n====\n\n[example]\nuntitled\n",
        &Options::default(),
    )?;
    let mut document = Document::default();
    document.attributes = parsed.document().attributes.clone();
    document.blocks = parsed.document().blocks.clone();
    assert_eq!(ordinals(&document), vec![Some(1), Some(2), Some(3), None]);

    // Removing a numbered block closes the gap it left.
    document.blocks.remove(0);
    document.renumber_captions();
    assert_eq!(ordinals(&document), vec![Some(1), Some(2), None]);

    // Reordering renumbers by tree order, not source order: after the swap the block that sits
    // later in the source holds the first ordinal.
    document.blocks.swap(0, 1);
    document.renumber_captions();
    let placed: Vec<_> = document
        .blocks
        .iter()
        .filter_map(|block| {
            let Block::DelimitedBlock(delimited) = block else {
                return None;
            };
            numbered(block)
                .and_then(|(_, _, number)| number)
                .map(|number| (delimited.location.absolute_start, number))
        })
        .collect();
    let [(first_start, first_number), (second_start, second_number)] = placed.as_slice() else {
        return Err(format!("expected two numbered blocks, got {placed:?}").into());
    };
    assert!(first_start > second_start, "{placed:?}");
    assert_eq!((*first_number, *second_number), (1, 2));

    // A title added after parsing earns an ordinal on the next run.
    let Some(Block::Paragraph(paragraph)) = document.blocks.last_mut() else {
        return Err(format!("expected the untitled paragraph, got {:?}", document.blocks).into());
    };
    paragraph.title = Title::new(vec![InlineNode::PlainText(Plain {
        content: "Added later",
        location: Location::default(),
        escaped: false,
    })]);
    assert_eq!(ordinals(&document), vec![Some(1), Some(2), None]);
    document.renumber_captions();
    assert_eq!(ordinals(&document), vec![Some(1), Some(2), Some(3)]);

    // Running it again changes nothing.
    let before = ordinals(&document);
    document.renumber_captions();
    assert_eq!(before, ordinals(&document));
    Ok(())
}

#[test]
fn caption_labels_keep_their_quote_characters() -> Result<(), Error> {
    // A document attribute's value is literal text, and the block-attribute parser has already
    // removed the syntactic quotes around an element value. Asciidoctor renders these as
    // `'Sample' 1.` and `'Snippet': `.
    let parsed = parse(
        "= T\n:example-caption: 'Sample'\n:listing-caption: \"Code\"\n\n.A\n[example]\none\n\n.B\n[listing]\ntwo\n\n.C\n[listing,caption=\"'Snippet': \"]\nthree\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    let labels: Vec<_> = document
        .blocks
        .iter()
        .filter_map(|block| match caption(block)? {
            Caption::Numbered { label, .. } | Caption::Custom(label) => Some(label.to_string()),
            Caption::Unnumbered | _ => None,
        })
        .collect();

    assert_eq!(
        labels,
        vec![
            "'Sample'".to_string(),
            "\"Code\"".to_string(),
            "'Snippet': ".to_string(),
        ]
    );
    Ok(())
}

#[test]
fn a_bare_caption_marker_is_ignored() -> Result<(), Error> {
    // `[listing,caption]` leaves a stray positional that asciidoctor ignores; only `caption=`
    // gives an empty custom caption. A bare marker must not swallow the label or the ordinal.
    let parsed = parse(
        "= T\n:listing-caption: Listing\n\n.Bare marker\n[listing,caption]\none\n\n.Next\n[listing]\ntwo\n\n.Empty value\n[listing,caption=]\nthree\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    let captions: Vec<_> = document
        .blocks
        .iter()
        .filter_map(caption)
        .cloned()
        .collect();

    assert_eq!(
        captions,
        vec![
            Caption::Numbered {
                kind: CaptionKind::Listing,
                label: "Listing".into(),
                number: NonZeroU32::new(1),
            },
            Caption::Numbered {
                kind: CaptionKind::Listing,
                label: "Listing".into(),
                number: NonZeroU32::new(2),
            },
            Caption::Custom("".into()),
        ]
    );
    Ok(())
}

#[test]
fn caller_built_metadata_carries_no_caption() -> Result<(), Error> {
    // Nothing knows the attributes at a source position a block never had, so a converter's own
    // fallback owns it.
    assert_eq!(BlockMetadata::default().caption, None);

    let parsed = parse("= T\n\n.Parsed\n====\nc\n====\n", &Options::default())?;
    let document = parsed.document();
    let block = document.blocks.first().ok_or("expected a block")?;
    assert!(caption(block).is_some());
    assert_eq!(document.highest_caption_number(CaptionKind::Example), 1);
    assert_eq!(document.highest_caption_number(CaptionKind::Listing), 0);
    Ok(())
}

#[test]
fn an_empty_element_caption_takes_no_prefix_and_no_ordinal() -> Result<(), Error> {
    let parsed = parse(
        "= T\n\n.Empty\n[example,caption=]\none\n\n.After\n[example]\ntwo\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    let [empty, after] = document.blocks.as_slice() else {
        return Err(format!("expected two blocks, got {:?}", document.blocks).into());
    };

    assert_eq!(caption(empty), Some(&Caption::Custom("".into())));
    assert_eq!(numbered(after).and_then(|(_, _, n)| n), Some(1));
    Ok(())
}

#[test]
fn caption_labels_come_from_api_supplied_attributes() -> Result<(), Error> {
    let mut attributes = acdc_parser::DocumentAttributes::default();
    attributes.set("example-caption".into(), AttributeValue::from("Sample"));
    let parsed = parse(
        "= T\n\n.Titled\n====\nc\n====\n",
        &Options::builder().with_attributes(attributes).build(),
    )?;
    let document = parsed.document();
    let block = document.blocks.first().ok_or("expected a block")?;
    assert_eq!(
        numbered(block),
        Some((CaptionKind::Example, "Sample".to_string(), Some(1)))
    );
    Ok(())
}

/// Each container that can hold blocks holds a captioned one, checked one container at a
/// time so a gap cannot hide behind another container's number.
///
/// This pins the *read-back* walk (`visit_captions`, behind `Document::highest_caption_number`).
/// Each document below has exactly one numbered block, so a container missing from that walk
/// reads back 0 rather than 1. Checking them together would not work: the read-back returns the
/// highest number, not a count, so a missing container is invisible unless it happened to hold
/// the highest one.
#[test]
fn every_container_is_read_back() -> Result<(), Error> {
    let containers: [(&str, &str); 14] = [
        ("top level", "= T\n\n.T\n[example]\none\n"),
        ("section", "= T\n\n== Section\n\n.T\n[example]\none\n"),
        (
            "admonition",
            "= T\n\n[NOTE]\n====\n.T\n[example]\none\n====\n",
        ),
        (
            "unordered list item",
            "= T\n\n* Item\n+\n.T\n[example]\none\n",
        ),
        (
            "ordered list item",
            "= T\n\n. Item\n+\n.T\n[example]\none\n",
        ),
        (
            "callout list item",
            "= T\n\n----\ncode <1>\n----\n<1> Callout\n+\n.T\n[example]\none\n",
        ),
        (
            "description list item",
            "= T\n\nterm:: description\n+\n.T\n[example]\none\n",
        ),
        ("example block", "= T\n\n====\n.T\n[example]\none\n====\n"),
        ("open block", "= T\n\n--\n.T\n[example]\none\n--\n"),
        ("sidebar block", "= T\n\n****\n.T\n[example]\none\n****\n"),
        ("quote block", "= T\n\n____\n.T\n[example]\none\n____\n"),
        (
            "table header cell",
            "= T\n\n[cols=\"1a\",options=\"header,footer\"]\n|===\n|.T\n[example]\none\n\n|body\n\n|footer\n|===\n",
        ),
        (
            "table body cell",
            "= T\n\n[cols=\"1a\",options=\"header,footer\"]\n|===\n|header\n\n|.T\n[example]\none\n\n|footer\n|===\n",
        ),
        (
            "table footer cell",
            "= T\n\n[cols=\"1a\",options=\"header,footer\"]\n|===\n|header\n\n|body\n\n|.T\n[example]\none\n|===\n",
        ),
    ];

    for (container, source) in containers {
        let parsed = parse(source, &Options::default())?;
        assert_eq!(
            parsed
                .document()
                .highest_caption_number(CaptionKind::Example),
            1,
            "a captioned block in a {container} is not reached by `visit_captions` in \
             model/caption.rs"
        );
    }
    Ok(())
}

/// The same containers, all in one document, pinning the *numbering* walk
/// (`renumber_captions`). Ordinals are handed out in sequence, so a container that walk never
/// reaches leaves every later block one number short and the highest number falls below the
/// count written below.
///
/// The expected values are literal counts of the captioned blocks in the source, deliberately
/// so that this test does not repeat the traversal it is checking.
#[test]
fn every_container_is_numbered() -> Result<(), Error> {
    const EXAMPLES_IN_SOURCE: u32 = 14;
    const LISTINGS_IN_SOURCE: u32 = 2;

    let source = "\
= Every container
:listing-caption: Listing

.Top level
[example]
top

.Top listing
[listing]
top listing

[NOTE]
====
.In admonition
[example]
admonition
====

* Item
+
.In unordered list
[example]
unordered

. Item
+
.In ordered list
[example]
ordered

term:: description
+
.In description list
[example]
description

----
code <1>
----
<1> Callout
+
.In callout list
[example]
callout

====
.In example block
[example]
example
====

--
.In open block
[example]
open
--

****
.In sidebar
[example]
sidebar
****

____
.In quote
[example]
quote
____

== Section

.In section
[example]
section

.In section listing
[listing]
section listing

[cols=\"1a\",options=\"header,footer\"]
|===
|.In table header
[example]
header

|.In table body
[example]
body

|.In table footer
[example]
footer
|===
";

    let parsed = parse(source, &Options::default())?;
    let document = parsed.document();

    assert_eq!(
        document.highest_caption_number(CaptionKind::Example),
        EXAMPLES_IN_SOURCE,
        "a container is missing from `renumber_captions` in model/caption.rs"
    );
    assert_eq!(
        document.highest_caption_number(CaptionKind::Listing),
        LISTINGS_IN_SOURCE,
        "a container is missing from `renumber_captions` in model/caption.rs"
    );
    Ok(())
}
