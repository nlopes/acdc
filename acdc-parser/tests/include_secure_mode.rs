use acdc_parser::{Block, InlineMacro, InlineNode, Options, SafeMode, parse_file};

#[test]
fn secure_mode_preserves_local_and_uri_includes_without_reading_them()
-> Result<(), Box<dyn std::error::Error>> {
    let options = Options::builder()
        .with_safe_mode(SafeMode::Secure)
        .with_attribute("allow-uri-read", true)
        .build();
    let result = parse_file("fixtures/preprocessor/secure_include_main.adoc", &options)?;

    let expected_targets = [
        "include_quote_part.adoc",
        "https://example.invalid/secret.adoc",
    ];
    assert_eq!(result.document().blocks.len(), expected_targets.len());
    for (block, expected_target) in result.document().blocks.iter().zip(expected_targets) {
        let Block::Paragraph(paragraph) = block else {
            return Err(std::io::Error::other(format!(
                "expected fallback paragraph, got {block:?}"
            ))
            .into());
        };
        let [InlineNode::Macro(InlineMacro::Link(link))] = paragraph.content.as_slice() else {
            return Err(std::io::Error::other(format!(
                "expected one fallback link, got {:?}",
                paragraph.content
            ))
            .into());
        };

        assert_eq!(link.target.to_string(), expected_target);
        assert!(link.text.is_empty());
        assert_eq!(link.attributes.iter().count(), 1);
        assert_eq!(
            link.attributes.get_string("role").as_deref(),
            Some("include")
        );
    }
    assert!(result.warnings().is_empty());

    Ok(())
}
