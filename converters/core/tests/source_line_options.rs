use acdc_converters_core::code::SourceLineOptions;
use acdc_parser::{
    AttributeValue, CompatibilityMode, Location, Options, Paragraph, ast::ParagraphRef, parse,
};

fn resolve(paragraph: &Paragraph<'_>, source: &str) -> SourceLineOptions {
    let options =
        SourceLineOptions::from_attributes(ParagraphRef::from(paragraph).attributes(), source);
    assert_eq!(
        options,
        SourceLineOptions::resolve(&paragraph.metadata, source)
    );
    options
}

#[test]
fn source_line_controls_require_the_exact_source_style() {
    for style in [
        None,
        Some(""),
        Some("listing"),
        Some("literal"),
        Some("Source"),
        Some("source"),
    ] {
        let mut paragraph = Paragraph::new(Vec::new(), Location::default());
        paragraph.metadata.style = style;
        paragraph
            .metadata
            .attributes
            .set("style".into(), "source".into());
        paragraph
            .metadata
            .attributes
            .set("linenums".into(), AttributeValue::None);
        paragraph
            .metadata
            .attributes
            .set("start".into(), "10".into());
        paragraph
            .metadata
            .attributes
            .set("highlight".into(), "10;12".into());
        let options = resolve(&paragraph, "one\ntwo\nthree\n");
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
}

#[test]
fn source_numbering_uses_presence_and_exact_start_text() {
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
            let mut paragraph = Paragraph::new(Vec::new(), Location::default());
            paragraph.metadata.style = Some("source");
            paragraph
                .metadata
                .attributes
                .set("linenums".into(), linenums);
            if let Some(start) = &start {
                paragraph
                    .metadata
                    .attributes
                    .set("start".into(), start.clone());
            }
            assert_eq!(
                resolve(&paragraph, "line").line_number_start,
                Some(expected)
            );
            paragraph.metadata.attributes.remove("linenums");
            assert_eq!(resolve(&paragraph, "line").line_number_start, None);
            paragraph.metadata.options.push("linenums");
            assert_eq!(
                resolve(&paragraph, "line").line_number_start,
                Some(expected)
            );
        }
    }
}

#[test]
fn highlight_selectors_preserve_order_separators_and_number_offsets() {
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
        let mut paragraph = Paragraph::new(Vec::new(), Location::default());
        paragraph.metadata.style = Some("source");
        paragraph
            .metadata
            .attributes
            .set("highlight".into(), selector.into());
        if let Some(start) = start {
            paragraph.metadata.options.push("linenums");
            paragraph
                .metadata
                .attributes
                .set("start".into(), start.into());
        }
        let options = resolve(&paragraph, "α\nbeta\ngamma\ndelta\n\n");
        assert_eq!(options.highlighted_lines, expected, "{start:?} {selector}");
    }
    for value in [
        AttributeValue::None,
        AttributeValue::Bool(false),
        AttributeValue::Bool(true),
    ] {
        let mut paragraph = Paragraph::new(Vec::new(), Location::default());
        paragraph.metadata.style = Some("source");
        paragraph.metadata.attributes.set("highlight".into(), value);
        assert!(resolve(&paragraph, "one\ntwo").is_empty());
    }
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
                            .metadata()
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
