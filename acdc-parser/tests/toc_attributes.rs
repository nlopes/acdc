use acdc_parser::{Block, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

// Expected text was captured from Asciidoctor 2.0.26 HTML output.
#[rstest::rstest]
#[case("", "[{toc}] [{toc-position}] [auto] [{toc-class}]")]
#[case(":toc:\n", "[] [{toc-position}] [auto] [{toc-class}]")]
#[case(":toc: left\n", "[] [left] [auto] [toc2]")]
#[case(":toc: >\n", "[] [right] [auto] [toc2]")]
#[case(":toc: macro\n", "[] [content] [macro] [{toc-class}]")]
#[case(":toc: preamble\n", "[] [content] [preamble] [{toc-class}]")]
#[case(":toc: other\n", "[] [{toc-position}] [auto] [{toc-class}]")]
#[case(":toc2:\n", "[] [left] [auto] [toc2]")]
#[case(":toc: left\n:toc-position: right\n", "[] [right] [auto] [toc2]")]
#[case(
    ":toc: left\n:toc-placement: macro\n",
    "[] [content] [macro] [{toc-class}]"
)]
#[case(":toc: left\n:toc-class: custom\n", "[] [left] [auto] [custom]")]
#[case(
    ":toc-placement!:\n",
    "[{toc}] [{toc-position}] [{toc-placement}] [{toc-class}]"
)]
#[case(":toc: left\n:toc-placement!:\n", "[] [content] [macro] [{toc-class}]")]
fn toc_header_normalization(#[case] header: &str, #[case] expected: &str) -> Result<(), Error> {
    // The final period keeps the probe from being parsed as block metadata.
    let input = format!(
        "= T\n{header}\n[{{toc}}] [{{toc-position}}] [{{toc-placement}}] [{{toc-class}}].\n"
    );
    let parsed = parse(&input, &Options::default())?;
    assert_eq!(
        paragraphs(&parsed.document().blocks),
        [format!("{expected}.")]
    );
    assert!(!parsed.document().attributes.contains_key("toc2"));
    Ok(())
}

#[rstest::rstest]
#[case(false, "[] [left] [auto]", "[] [left] [auto]")]
#[case(true, "[] [right] [auto]", "[macro] [right] [auto]")]
fn toc_normalization_preserves_caller_precedence(
    #[case] soft: bool,
    #[case] first: &str,
    #[case] last: &str,
) -> Result<(), Error> {
    let builder = Options::builder();
    let options = if soft {
        builder.with_default_attribute("toc", "left")
    } else {
        builder.with_attribute("toc", "left")
    }
    .build()?;
    let parsed = parse(
        "= T\n:toc: right\n\nFirst: [{toc}] [{toc-position}] [{toc-placement}].\n\n:toc: macro\n\nLast: [{toc}] [{toc-position}] [{toc-placement}].\n",
        &options,
    )?;
    assert_eq!(
        paragraphs(&parsed.document().blocks),
        [format!("First: {first}."), format!("Last: {last}.")]
    );
    Ok(())
}

#[test]
fn toc_normalization_preserves_caller_unset() -> Result<(), Error> {
    let parsed = parse(
        "= T\n:toc: right\n\nFirst: [{toc}] [{toc-position}] [{toc-placement}].\n\n:toc: macro\n\nLast: [{toc}] [{toc-position}] [{toc-placement}].\n",
        &Options::builder().with_attribute("toc", ()).build()?,
    )?;
    assert_eq!(
        paragraphs(&parsed.document().blocks),
        [
            "First: [{toc}] [{toc-position}] [auto].",
            "Last: [{toc}] [{toc-position}] [auto]."
        ]
    );
    Ok(())
}

#[test]
fn toc_normalization_stops_at_header_boundary() -> Result<(), Error> {
    let parsed = parse(
        "= T\n:toc: left\n\nFirst.\n\n:toc: right\n\n[{toc}] [{toc-position}] [{toc-placement}] [{toc-class}].\n",
        &Options::default(),
    )?;
    assert_eq!(
        paragraphs(&parsed.document().blocks),
        ["First.", "[right] [left] [auto] [toc2]."]
    );
    Ok(())
}

fn paragraphs(blocks: &[Block<'_>]) -> Vec<String> {
    blocks
        .iter()
        .filter_map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return None;
            };
            Some(
                paragraph
                    .content
                    .iter()
                    .filter_map(|node| {
                        let InlineNode::PlainText(text) = node else {
                            return None;
                        };
                        Some(text.content)
                    })
                    .collect(),
            )
        })
        .collect()
}
