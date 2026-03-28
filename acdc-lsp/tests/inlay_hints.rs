//! Integration tests for `textDocument/inlayHint`.

mod common;

use common::LspTestClient;
use serde_json::json;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn test_initialize_advertises_inlay_hints() -> TestResult {
    let mut client = LspTestClient::new()?;
    let result = client.initialize()?;

    let provider = result
        .get("capabilities")
        .and_then(|c| c.get("inlayHintProvider"));
    assert_eq!(
        provider,
        Some(&json!(true)),
        "Expected inlayHintProvider: true, got {provider:?}"
    );

    client.shutdown();
    Ok(())
}

#[test]
fn test_inlay_hint_attribute() -> TestResult {
    let mut client = LspTestClient::new()?;
    client.initialize()?;

    client.open_document(
        "file:///test.adoc",
        ":product-name: Acme Cloud Platform\n\n== Section\n\nWelcome to {product-name} docs.\n",
    )?;

    let result = client.send_request(
        "textDocument/inlayHint",
        json!({
            "textDocument": { "uri": "file:///test.adoc" },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 10, "character": 0 }
            }
        }),
    )?;

    let hints = result.as_array().ok_or("expected array response")?;
    assert!(
        !hints.is_empty(),
        "Expected at least one inlay hint for attribute reference"
    );

    // Find the attribute hint
    let attr_hint = hints
        .iter()
        .find(|h| {
            h.get("label")
                .and_then(|l| l.as_str())
                .is_some_and(|s| s.contains("Acme Cloud Platform"))
        })
        .ok_or("Expected hint containing 'Acme Cloud Platform'")?;

    assert_eq!(
        attr_hint.get("paddingLeft"),
        Some(&json!(true)),
        "Expected paddingLeft: true"
    );

    client.shutdown();
    Ok(())
}

#[test]
fn test_inlay_hint_xref() -> TestResult {
    let mut client = LspTestClient::new()?;
    client.initialize()?;

    client.open_document(
        "file:///test.adoc",
        "[[setup]]\n== Initial Setup\n\nSee <<setup>> for details.\n",
    )?;

    let result = client.send_request(
        "textDocument/inlayHint",
        json!({
            "textDocument": { "uri": "file:///test.adoc" },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 10, "character": 0 }
            }
        }),
    )?;

    let hints = result.as_array().ok_or("expected array response")?;

    let xref_hint = hints.iter().find(|h| {
        h.get("label")
            .and_then(|l| l.as_str())
            .is_some_and(|s| s.contains("Initial Setup"))
    });
    assert!(
        xref_hint.is_some(),
        "Expected hint containing 'Initial Setup', got: {hints:?}"
    );

    client.shutdown();
    Ok(())
}
