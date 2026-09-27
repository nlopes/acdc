use acdc_parser::{Block, InlineMacro, InlineNode, Options, SafeMode, parse_file};

#[test]
fn secure_mode_preserves_local_and_uri_includes_without_reading_them()
-> Result<(), Box<dyn std::error::Error>> {
    let options = Options::builder()
        .with_safe_mode(SafeMode::Secure)
        .with_attribute("allow-uri-read", true)
        .build()?;
    let result = parse_file("fixtures/preprocessor/secure_include_main.adoc", &options)?;

    let expected_targets = [
        "include_quote_part.adoc",
        "https://example.invalid/secret.adoc",
    ];
    assert_eq!(result.document().blocks.len(), expected_targets.len());
    for ((block, expected_target), expected_line) in result
        .document()
        .blocks
        .iter()
        .zip(expected_targets)
        .zip([3, 5])
    {
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
        assert_eq!(paragraph.location.start.line, expected_line);
        assert!(paragraph.location.start.file.is_none());
    }
    assert_eq!(result.warnings().len(), 2);
    for (warning, line) in result.warnings().iter().zip([3, 5]) {
        assert!(matches!(
            warning.kind,
            acdc_parser::WarningKind::ContentRecovery { .. }
        ));
        let location = warning
            .source_location()
            .ok_or("missing recovery location")?;
        assert_eq!(location.location.start.line, line);
        assert_eq!(
            location
                .file
                .as_deref()
                .and_then(std::path::Path::file_name),
            Some(std::ffi::OsStr::new("secure_include_main.adoc"))
        );
    }

    Ok(())
}
