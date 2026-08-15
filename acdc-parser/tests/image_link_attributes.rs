use acdc_parser::{Block, InlineMacro, InlineNode, Options, parse, parse_inline};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn inline_image_link(input: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let parsed = parse_inline(input, &Options::default())?;
    let [InlineNode::Macro(InlineMacro::Image(image))] = parsed.inlines() else {
        return Err(format!("expected one inline image, got {:?}", parsed.inlines()).into());
    };
    Ok(image
        .metadata
        .attributes
        .get("link")
        .map(std::string::ToString::to_string))
}

#[test]
fn empty_inline_image_link_is_preserved() -> TestResult {
    assert_eq!(
        inline_image_link("image:photo.png[Photo,link=]")?,
        Some(String::new())
    );
    Ok(())
}

#[test]
fn repeated_inline_image_link_uses_the_last_value() -> TestResult {
    assert_eq!(
        inline_image_link(
            "image:photo.png[Photo,link=https://example.com/first,link=https://example.com/last]"
        )?,
        Some("https://example.com/last".to_string())
    );
    Ok(())
}

#[test]
fn repeated_block_image_link_uses_the_last_value() -> TestResult {
    let parsed = parse(
        "image::photo.png[Photo,link=https://example.com/first,link=https://example.com/last]",
        &Options::default(),
    )?;
    let [Block::Image(image)] = parsed.document().blocks.as_slice() else {
        return Err(format!(
            "expected one block image, got {:?}",
            parsed.document().blocks
        )
        .into());
    };

    assert_eq!(
        image
            .metadata
            .attributes
            .get("link")
            .map(ToString::to_string),
        Some("https://example.com/last".to_string())
    );
    Ok(())
}
