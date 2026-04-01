//! Integration tests for `textDocument/selectionRange`.

mod common;

use common::LspTestClient;
use serde_json::json;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn test_initialize_advertises_selection_range() -> TestResult {
    let mut client = LspTestClient::new()?;
    let result = client.initialize()?;

    let provider = result
        .get("capabilities")
        .and_then(|c| c.get("selectionRangeProvider"));
    assert_eq!(
        provider,
        Some(&json!(true)),
        "Expected selectionRangeProvider: true, got {provider:?}"
    );

    client.shutdown();
    Ok(())
}

#[test]
fn test_selection_range_basic() -> TestResult {
    let mut client = LspTestClient::new()?;
    client.initialize()?;

    client.open_document(
        "file:///test.adoc",
        "= Title\n\n== Section\n\nSome *bold* text.\n",
    )?;

    // Position on "bold" (line 4, character 6 — inside the bold markup)
    let result = client.send_request(
        "textDocument/selectionRange",
        json!({
            "textDocument": { "uri": "file:///test.adoc" },
            "positions": [{ "line": 4, "character": 6 }]
        }),
    )?;

    let ranges = result.as_array().ok_or("expected array response")?;
    assert_eq!(ranges.len(), 1);

    // Should have a parent chain (at minimum: inline → paragraph → section → document)
    let first = ranges.first().ok_or("expected at least one range")?;
    assert!(
        first.get("parent").is_some(),
        "Expected parent chain, got: {first}"
    );

    client.shutdown();
    Ok(())
}

#[test]
fn test_selection_range_multiple_positions() -> TestResult {
    let mut client = LspTestClient::new()?;
    client.initialize()?;

    client.open_document(
        "file:///test.adoc",
        "= Title\n\nFirst paragraph.\n\nSecond paragraph.\n",
    )?;

    let result = client.send_request(
        "textDocument/selectionRange",
        json!({
            "textDocument": { "uri": "file:///test.adoc" },
            "positions": [
                { "line": 2, "character": 0 },
                { "line": 4, "character": 0 }
            ]
        }),
    )?;

    let ranges = result.as_array().ok_or("expected array response")?;
    assert_eq!(ranges.len(), 2, "Expected one selection range per position");

    client.shutdown();
    Ok(())
}

#[test]
fn test_selection_range_nested_inline() -> TestResult {
    let mut client = LspTestClient::new()?;
    client.initialize()?;

    client.open_document("file:///test.adoc", "= Title\n\n*_bold italic_*\n")?;

    // Position inside the italic text (line 2, character 3)
    let result = client.send_request(
        "textDocument/selectionRange",
        json!({
            "textDocument": { "uri": "file:///test.adoc" },
            "positions": [{ "line": 2, "character": 3 }]
        }),
    )?;

    let ranges = result.as_array().ok_or("expected array response")?;
    assert_eq!(ranges.len(), 1);

    // Count the depth of the parent chain
    let mut depth = 1;
    let mut current = ranges.first().ok_or("expected at least one range")?;
    while let Some(parent) = current.get("parent") {
        depth += 1;
        current = parent;
    }

    // Should have at least 4 levels: plain text → italic → bold → paragraph → document
    assert!(
        depth >= 4,
        "Expected at least 4 levels of nesting for nested inline, got {depth}"
    );

    client.shutdown();
    Ok(())
}
