use acdc_parser::{Options, SafeMode, parse_file};

#[test]
fn secure_mode_preserves_local_and_uri_includes_without_reading_them()
-> Result<(), Box<dyn std::error::Error>> {
    let options = Options::builder()
        .with_safe_mode(SafeMode::Secure)
        .with_attribute("allow-uri-read", true)
        .build();
    let result = parse_file("fixtures/preprocessor/secure_include_main.adoc", &options)?;
    let json = serde_json::to_string(result.document())?;

    assert!(!json.contains("Part intro paragraph"));
    assert!(json.contains("include_quote_part.adoc"));
    assert!(json.contains("https://example.invalid/secret.adoc"));
    assert_eq!(json.matches("\"value\":\"role=include\"").count(), 2);
    assert!(result.warnings().is_empty());

    Ok(())
}
