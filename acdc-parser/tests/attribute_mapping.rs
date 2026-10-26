use std::path::Path;

use acdc_parser::{
    AttributeValue, Block, CompatibilityMode, DelimitedBlockType, Location, Options, SafeMode,
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case::asciidoctor(CompatibilityMode::Asciidoctor, r"Label\] with café")]
#[case::draft(CompatibilityMode::Draft, "Label] with café")]
fn block_anchor_bracket_escapes_follow_the_mode(
    #[case] mode: CompatibilityMode,
    #[case] expected: &str,
) -> Result<(), Error> {
    let source = "[[bracket,Label\\] with café]]\nText.\n\nSee <<bracket>>.";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(Block::Paragraph(block)) = parsed.document().blocks.first() else {
        return Err("expected a paragraph".into());
    };
    let anchor = block.metadata.anchors.first().ok_or("missing anchor")?;
    assert_eq!(anchor.xreflabel, Some(expected));
    assert!(parsed.document().references.contains_key("bracket"));
    Ok(())
}

#[test]
fn literal_attributions_exclude_the_following_delimiter() -> Result<(), Error> {
    for value in ["Ada", "café", "λ"] {
        for quote in ["", "\""] {
            let source = format!("[quote,{quote}{value}{quote},Book]\n____\nWords.\n____");
            for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
                let parsed = parse(
                    &source,
                    &Options::builder().with_compatibility_mode(mode).build()?,
                )?;
                let [Block::DelimitedBlock(block)] = parsed.document().blocks.as_slice() else {
                    return Err("expected a quote block".into());
                };
                let attribution = block
                    .metadata
                    .attribution
                    .as_ref()
                    .ok_or("missing attribution")?;
                let [inline] = attribution.as_ref() else {
                    return Err("expected one attribution inline".into());
                };
                let location = inline.location();
                let end = source[location.absolute_end..]
                    .chars()
                    .next()
                    .ok_or("missing end character")?;
                assert_eq!(
                    source.get(location.absolute_start..location.absolute_end + end.len_utf8()),
                    Some(value)
                );
            }
        }
    }
    Ok(())
}

#[test]
fn expanded_attributions_point_to_the_authored_reference() -> Result<(), Error> {
    let source = ":who: café\n\n[quote,\"{who}\",Book]\n____\nWords.\n____";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let [Block::DelimitedBlock(block)] = parsed.document().blocks.as_slice() else {
            return Err("expected a quote block".into());
        };
        let attribution = block
            .metadata
            .attribution
            .as_ref()
            .ok_or("missing attribution")?;
        let [inline] = attribution.as_ref() else {
            return Err("expected one attribution inline".into());
        };
        let location = inline.location();
        assert_eq!(
            source.get(location.absolute_start..=location.absolute_end),
            Some("{who}")
        );
        assert_eq!(location.start.line, 3);
    }
    Ok(())
}

#[rstest::rstest]
#[case::asciidoctor(CompatibilityMode::Asciidoctor, r"\{who}")]
#[case::draft(CompatibilityMode::Draft, r"\Ada")]
fn even_backslashes_before_a_reference_follow_the_mode(
    #[case] mode: CompatibilityMode,
    #[case] expected: &str,
) -> Result<(), Error> {
    let source = ":who: Ada\n\n[data=\\\\{who}]\nText.";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [Block::Paragraph(block)] = parsed.document().blocks.as_slice() else {
        return Err("expected a paragraph".into());
    };
    let Some(AttributeValue::String(value)) = block.metadata.attributes.get("data") else {
        return Err("missing data attribute".into());
    };
    assert_eq!(value, expected);
    Ok(())
}

#[test]
fn included_and_nested_values_keep_authored_byte_ranges() -> Result<(), Error> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/tests/compatibility_attribute_locations.adoc");
    let source = std::fs::read_to_string(&fixture)?;
    let included = fixture
        .parent()
        .ok_or("missing fixture directory")?
        .join("includes/compatibility_attribute_locations.adoc");
    let included_source = std::fs::read_to_string(&included)?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse_file(
            &fixture,
            &Options::builder()
                .with_compatibility_mode(mode)
                .with_safe_mode(SafeMode::Unsafe)
                .build()?,
        )?;
        let blocks = &parsed.document().blocks;
        let location = attribution_location(blocks.get(5).ok_or("missing included quote")?)?;
        assert_eq!(
            included_source.get(location.absolute_start..=location.absolute_end),
            Some("{author}")
        );
        assert_eq!(location.start.line, 2);
        assert_eq!(location.start.column, 9);
        assert_eq!(location.end.column, 16);
        assert!(
            location
                .start
                .file
                .as_ref()
                .and_then(|files| files.last())
                .is_some_and(
                    |file| file.ends_with("includes/compatibility_attribute_locations.adoc")
                )
        );

        let Some(Block::DelimitedBlock(block)) = blocks.last() else {
            return Err("missing table block".into());
        };
        let DelimitedBlockType::DelimitedTable(table) = &block.inner else {
            return Err("expected a table".into());
        };
        let quote = table
            .rows
            .first()
            .and_then(|row| row.columns.first())
            .and_then(|column| column.content.first())
            .ok_or("missing table quote")?;
        let location = attribution_location(quote)?;
        assert_eq!(
            source.get(location.absolute_start..=location.absolute_end),
            Some("{author}")
        );
        assert_eq!(location.start.line, 34);
        assert_eq!(location.start.column, 10);
        assert_eq!(location.end.column, 17);
        assert!(location.start.file.is_none());
    }
    Ok(())
}

fn attribution_location<'a>(block: &'a Block<'_>) -> Result<&'a Location, Error> {
    let Block::DelimitedBlock(block) = block else {
        return Err("expected a quote block".into());
    };
    block
        .metadata
        .attribution
        .as_ref()
        .and_then(|value| value.first())
        .map(acdc_parser::InlineNode::location)
        .ok_or_else(|| "missing attribution".into())
}

#[test]
fn block_anchor_reftext_expands_saved_references_once() -> Result<(), Error> {
    let source = ":next: changed\n:saved: \\{next}\n\n[[id,{saved}]]\nText.\n\nSee <<id>>.";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let Some(Block::Paragraph(block)) = parsed.document().blocks.first() else {
            return Err("expected a paragraph".into());
        };
        assert_eq!(
            block
                .metadata
                .anchors
                .first()
                .ok_or("missing anchor")?
                .xreflabel,
            Some("{next}")
        );
    }
    Ok(())
}
