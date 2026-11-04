use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef, InlineKind, InlineRef},
    parse, parse_inline,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn inline_image_link(input: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let parsed = parse_inline(input, &Options::default())?;
    let [InlineKind::Image(image)] = &*parsed
        .content()
        .inlines()
        .map(InlineRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err(format!("expected one inline image, got {:?}", parsed.content()).into());
    };
    Ok(image
        .attributes()
        .get("link")
        .and_then(|value| {
            if let acdc_parser::ast::AttributeValueRef::Text(text) = value {
                Some(text)
            } else {
                None
            }
        })
        .map(str::to_owned))
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
    let [BlockKind::Image(image)] = &*parsed
        .document_ref()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err(format!(
            "expected one block image, got {:?}",
            parsed.document_ref().blocks().collect::<Vec<_>>()
        )
        .into());
    };

    assert_eq!(
        image
            .attributes()
            .get("link")
            .and_then(
                |value| if let acdc_parser::ast::AttributeValueRef::Text(text) = value {
                    Some(text)
                } else {
                    None
                }
            )
            .map(str::to_owned),
        Some("https://example.com/last".to_string())
    );
    Ok(())
}
