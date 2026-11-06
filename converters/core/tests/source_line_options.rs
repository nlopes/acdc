//! Source line controls preserve numbering and highlights for source-style blocks.

use acdc_converters_core::code::SourceLineOptions;
use acdc_parser::{
    AttributeValue, BlockMetadata, CompatibilityMode, Location, Options, Paragraph,
    ast::ParagraphRef, parse,
};

type Error = Box<dyn std::error::Error>;

fn resolve(metadata: &BlockMetadata<'_>, source: &str) -> Result<SourceLineOptions, Error> {
    let mut paragraph = Paragraph::new(Vec::new(), Location::default());
    let mut prepared = metadata.clone();
    if prepared.attributes.remove("style").is_some() {
        assert!(paragraph.set_metadata(metadata.clone()).is_err());
    }
    paragraph.set_metadata(prepared)?;
    let options =
        SourceLineOptions::from_attributes(ParagraphRef::from(&paragraph).attributes(), source);
    assert_eq!(options, SourceLineOptions::resolve(metadata, source));
    Ok(options)
}

#[test]
fn source_line_controls_require_the_exact_source_style() -> Result<(), Error> {
    for style in [
        None,
        Some(""),
        Some("listing"),
        Some("literal"),
        Some("Source"),
        Some("source"),
    ] {
        let mut metadata = BlockMetadata::default();
        metadata.style = style;
        metadata.attributes.set("style".into(), "source".into());
        metadata
            .attributes
            .set("linenums".into(), AttributeValue::None);
        metadata.attributes.set("start".into(), "10".into());
        metadata.attributes.set("highlight".into(), "10;12".into());
        let options = resolve(&metadata, "one\ntwo\nthree\n")?;
        assert_eq!(
            options,
            if style == Some("source") {
                SourceLineOptions {
                    line_number_start: Some(10),
                    highlighted_lines: vec![1, 3],
                }
            } else {
                SourceLineOptions::default()
            }
        );
    }
    Ok(())
}

#[test]
fn source_numbering_uses_presence_and_exact_start_text() -> Result<(), Error> {
    let values = [
        (None, 1),
        (Some(AttributeValue::None), 1),
        (Some(AttributeValue::Bool(false)), 1),
        (Some(AttributeValue::from("")), 1),
        (Some(AttributeValue::from("0")), 1),
        (Some(AttributeValue::from("-1")), 1),
        (Some(AttributeValue::from(" 10 ")), 1),
        (Some(AttributeValue::from("'10'")), 1),
        (
            Some(AttributeValue::from("999999999999999999999999999999")),
            1,
        ),
        (Some(AttributeValue::from("10")), 10),
    ];
    for (start, expected) in values {
        for linenums in [
            AttributeValue::None,
            AttributeValue::Bool(false),
            AttributeValue::Bool(true),
            "".into(),
            "false".into(),
        ] {
            let mut metadata = BlockMetadata::default();
            metadata.style = Some("source");
            metadata.attributes.set("linenums".into(), linenums);
            if let Some(start) = &start {
                metadata.attributes.set("start".into(), start.clone());
            }
            assert_eq!(
                resolve(&metadata, "line")?.line_number_start,
                Some(expected)
            );
            metadata.attributes.remove("linenums");
            assert_eq!(resolve(&metadata, "line")?.line_number_start, None);
            metadata.options.push("linenums");
            assert_eq!(
                resolve(&metadata, "line")?.line_number_start,
                Some(expected)
            );
        }
    }
    Ok(())
}

#[test]
fn highlight_selectors_preserve_order_separators_and_number_offsets() -> Result<(), Error> {
    for (start, selector, expected) in [
        (None, "1..4,!2,2", vec![1, 2, 3, 4]),
        (None, "1;3;!1", vec![3]),
        (None, " 1 - 3 , ! 2 ", vec![1, 3]),
        (None, "1..", vec![1, 2, 3, 4]),
        (None, "0,99,garbage", vec![]),
        (None, "'1'", vec![]),
        (Some("10"), "10;12", vec![1, 3]),
        (Some("10"), "10..12,!11", vec![1, 3]),
    ] {
        let mut metadata = BlockMetadata::default();
        metadata.style = Some("source");
        metadata.attributes.set("highlight".into(), selector.into());
        if let Some(start) = start {
            metadata.options.push("linenums");
            metadata.attributes.set("start".into(), start.into());
        }
        let options = resolve(&metadata, "α\nbeta\ngamma\ndelta\n\n")?;
        assert_eq!(options.highlighted_lines, expected, "{start:?} {selector}");
    }
    for value in [
        AttributeValue::None,
        AttributeValue::Bool(false),
        AttributeValue::Bool(true),
    ] {
        let mut metadata = BlockMetadata::default();
        metadata.style = Some("source");
        metadata.attributes.set("highlight".into(), value);
        assert!(resolve(&metadata, "one\ntwo")?.is_empty());
    }
    Ok(())
}

#[test]
fn parsed_source_line_options_keep_modes_and_block_forms() -> Result<(), Box<dyn std::error::Error>>
{
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for body in ["one\ntwo\nthree", "----\none\ntwo\nthree\n----"] {
            let source = format!("[source,rust,linenums,start=10,highlight=\"10;12\"]\n{body}\n");
            let options = {
                let parsed = parse(
                    &source,
                    &Options::builder().with_compatibility_mode(mode).build()?,
                )?;
                let parsed_blocks: Vec<_> = parsed
                    .document()
                    .blocks()
                    .map(acdc_parser::Block::from)
                    .collect();
                let block = parsed.document().blocks().next().ok_or("source block")?;
                let attributes = block.attributes().ok_or("attributes")?;
                let options = SourceLineOptions::from_attributes(attributes, "one\ntwo\nthree");
                assert_eq!(
                    options,
                    SourceLineOptions::resolve(
                        parsed_blocks
                            .first()
                            .ok_or("owner")?
                            .clone_metadata()
                            .as_ref()
                            .ok_or("metadata")?,
                        "one\ntwo\nthree"
                    )
                );
                options
            };
            assert_eq!(
                options,
                SourceLineOptions {
                    line_number_start: Some(10),
                    highlighted_lines: vec![1, 3]
                }
            );
        }
    }
    Ok(())
}
