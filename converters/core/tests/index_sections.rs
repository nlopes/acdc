//! Index discovery boundaries and compatibility with existing style queries.

use acdc_converters_core::section::{
    contains_index_section, has_index_section, last_section_has_style,
};
use acdc_parser::{
    Admonition, AdmonitionVariant, Block, ColumnFormat, ColumnStyle, CompatibilityMode,
    DelimitedBlock, DelimitedBlockType, Location, Options, Section, SectionKind, Table, TableRow,
    Title, ast::BlockRef, parse,
};

type Error = Box<dyn std::error::Error>;

fn assert_detection(blocks: &[Block<'_>], expected: bool) {
    assert_eq!(has_index_section(blocks), expected);
    assert_eq!(
        contains_index_section(blocks.iter().map(BlockRef::from)),
        expected
    );
}

fn index_section() -> Result<Block<'static>, Error> {
    let mut section = Section::new(Title::default(), 1, Vec::new(), Location::default());
    let mut metadata = section.take_metadata();
    metadata.style = SectionKind::Index.as_style();
    section.set_metadata(metadata)?;
    Ok(Block::Section(section))
}

#[test]
fn index_detection_is_independent_of_last_section_style() -> Result<(), Error> {
    let parsed = parse(
        "= Manual\n\n[index]\n== Index\n\n[bibliography]\n== References\n",
        &Options::default(),
    )?;
    let parsed_blocks: Vec<_> = parsed
        .document()
        .blocks()
        .map(acdc_parser::Block::from)
        .collect();
    let blocks = &parsed_blocks;
    assert_detection(blocks, true);
    assert!(!last_section_has_style(blocks, "index"));
    assert!(last_section_has_style(blocks, "bibliography"));
    Ok(())
}

#[test]
fn index_detection_reaches_sections_inside_book_parts() -> Result<(), Error> {
    let parsed = parse(
        "= Manual\n:doctype: book\n\n= Part\n\n== Chapter\n\nText.\n\n[index]\n== Index\n\n= Later Part\n\n== Later Chapter\n",
        &Options::default(),
    )?;
    let parsed_blocks: Vec<_> = parsed
        .document()
        .blocks()
        .map(acdc_parser::Block::from)
        .collect();
    assert_detection(&parsed_blocks, true);
    Ok(())
}

#[test]
fn index_detection_excludes_styled_paragraphs_and_plain_cells() -> Result<(), Error> {
    for input in [
        "= Manual\n\n[index]\nParagraph.\n",
        "= Manual\n\n--\n[index]\n== Not a section\n--\n",
        "= Manual\n\n[cols=l]\n|===\n| [index]\n== Cell Index\n|===\n",
    ] {
        let parsed = parse(input, &Options::default())?;
        let parsed_blocks: Vec<_> = parsed
            .document()
            .blocks()
            .map(acdc_parser::Block::from)
            .collect();
        assert_detection(&parsed_blocks, false);
    }
    Ok(())
}

#[test]
fn index_detection_reaches_asciidoc_table_cells() -> Result<(), Error> {
    for columns in ["[cols=a]", "[cols=1]"] {
        let cell_prefix = if columns == "[cols=a]" { "|" } else { "a|" };
        let input =
            format!("= Manual\n\n{columns}\n|===\n{cell_prefix} [index]\n== Cell Index\n|===\n");
        for (prefix, suffix) in [
            ("", ""),
            ("--\n", "\n--"),
            ("[NOTE]\n====\n", "\n===="),
            ("* Item\n+\n", ""),
            ("Term:: Text\n+\n", ""),
        ] {
            let table = input
                .strip_prefix("= Manual\n\n")
                .ok_or("missing fixture header")?;
            let input = format!("= Manual\n\n{prefix}{table}{suffix}\n");
            let parsed = parse(&input, &Options::default())?;
            let parsed_blocks: Vec<_> = parsed
                .document()
                .blocks()
                .map(acdc_parser::Block::from)
                .collect();
            assert_detection(&parsed_blocks, true);
        }
    }
    Ok(())
}

#[test]
fn index_detection_preserves_constructed_compound_and_list_boundaries() -> Result<(), Error> {
    for inner in [
        DelimitedBlockType::DelimitedExample(vec![index_section()?]),
        DelimitedBlockType::DelimitedOpen(vec![index_section()?]),
        DelimitedBlockType::DelimitedQuote(vec![index_section()?]),
        DelimitedBlockType::DelimitedSidebar(vec![index_section()?]),
    ] {
        let block = Block::DelimitedBlock(DelimitedBlock::new(inner, "", Location::default()));
        assert_detection(&[block], true);
    }
    assert_detection(
        &[Block::Admonition(Admonition::new(
            AdmonitionVariant::Note,
            vec![index_section()?],
            Location::default(),
        ))],
        true,
    );
    let mut section = Section::new(
        Title::default(),
        1,
        vec![index_section()?],
        Location::default(),
    );
    assert_detection(&[Block::Section(section.clone())], true);
    section.set_blocks([]);
    assert_detection(&[Block::Section(section.clone())], false);
    let mut metadata = section.take_metadata();
    metadata.style = Some("index");
    section.set_metadata(metadata)?;
    assert_detection(&[Block::Section(section)], true);

    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for source in [
            "* Unordered item\n",
            ". Ordered item\n",
            "Term:: Description\n",
            "----\ncode <1>\n----\n\n<1> Explanation\n",
        ] {
            let parsed = parse(source, &options)?;
            let parsed_blocks: Vec<_> = parsed
                .document()
                .blocks()
                .map(acdc_parser::Block::from)
                .collect();
            let mut blocks = parsed_blocks.clone();
            assert_detection(&blocks, false);
            let list = blocks.last_mut().ok_or("missing list")?;
            let attachments = if let Block::UnorderedList(list) = list {
                &mut list
                    .items
                    .first_mut()
                    .ok_or("missing unordered item")?
                    .blocks
            } else if let Block::OrderedList(list) = list {
                &mut list.items.first_mut().ok_or("missing ordered item")?.blocks
            } else if let Block::DescriptionList(list) = list {
                &mut list
                    .items
                    .first_mut()
                    .ok_or("missing description item")?
                    .description
            } else if let Block::CalloutList(list) = list {
                &mut list.items.first_mut().ok_or("missing callout item")?.blocks
            } else {
                return Err(format!("expected a list in {source:?}").into());
            };
            attachments.push(index_section()?);
            assert_detection(&blocks, true);
        }
    }
    Ok(())
}

#[test]
fn index_detection_preserves_stored_cell_style_fallback_in_every_row_group() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = parse("|===\n| First | Second\n|===\n", &options)?;
        let parsed_blocks: Vec<_> = parsed
            .document()
            .blocks()
            .map(acdc_parser::Block::from)
            .collect();
        let Some(Block::DelimitedBlock(block)) = parsed_blocks.first() else {
            return Err("missing table block".into());
        };
        let DelimitedBlockType::DelimitedTable(seed) = &block.clone_body() else {
            return Err("missing table".into());
        };
        let mut row = seed.rows.first().ok_or("missing table row")?.clone();
        let [first, second] = row.columns.as_mut_slice() else {
            return Err("expected two table cells".into());
        };
        first.colspan = 2;
        first.style = Some(ColumnStyle::Default);
        second.content = vec![index_section()?];
        for fallback in [
            ColumnStyle::Default,
            ColumnStyle::AsciiDoc,
            ColumnStyle::Literal,
        ] {
            for local in [
                None,
                Some(ColumnStyle::Default),
                Some(ColumnStyle::AsciiDoc),
                Some(ColumnStyle::Literal),
            ] {
                row.columns.get_mut(1).ok_or("missing second cell")?.style = local;
                let expected = local.unwrap_or(fallback) == ColumnStyle::AsciiDoc;
                for group in 0..3 {
                    let mut table = Table::new(Vec::new(), Location::default()).with_columns(vec![
                        ColumnFormat::new().with_style(ColumnStyle::AsciiDoc),
                        ColumnFormat::new().with_style(fallback),
                        ColumnFormat::new().with_style(ColumnStyle::Default),
                    ]);
                    table.header = Some(TableRow::new(Vec::new()));
                    table.footer = Some(TableRow::new(Vec::new()));
                    match group {
                        0 => table.header = Some(row.clone()),
                        1 => table.rows.push(row.clone()),
                        _ => table.footer = Some(row.clone()),
                    }
                    let block = Block::DelimitedBlock(DelimitedBlock::new(
                        DelimitedBlockType::DelimitedTable(table),
                        "|===",
                        Location::default(),
                    ));
                    assert_detection(&[block], expected);
                }
            }
        }
        for local in [
            None,
            Some(ColumnStyle::Default),
            Some(ColumnStyle::AsciiDoc),
        ] {
            row.columns.get_mut(1).ok_or("missing second cell")?.style = local;
            let block = Block::DelimitedBlock(DelimitedBlock::new(
                DelimitedBlockType::DelimitedTable(Table::new(
                    vec![row.clone()],
                    Location::default(),
                )),
                "|===",
                Location::default(),
            ));
            assert_detection(&[block], local == Some(ColumnStyle::AsciiDoc));
        }
    }
    Ok(())
}

#[test]
fn typed_index_detection_accepts_short_lived_and_single_pass_inputs() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let found = {
            let source = String::from("= Manual\n\n[index]\n== Index\n");
            let parsed = parse(&source, &options)?;
            contains_index_section(parsed.document().blocks())
        };
        assert!(found);
    }
    let blocks = [
        Block::Section(Section::new(
            Title::default(),
            1,
            Vec::new(),
            Location::default(),
        )),
        index_section()?,
        index_section()?,
    ];
    let mut remaining = blocks.iter().map(BlockRef::from);
    let mut pulled = 0;
    let input = std::iter::from_fn(|| {
        let node = remaining.next()?;
        pulled += 1;
        Some(node)
    });
    assert!(contains_index_section(input));
    assert_eq!(pulled, 2);
    assert!(remaining.next().is_some());
    assert!(!contains_index_section(std::iter::empty()));
    Ok(())
}
