//! Integration tests for backend-aware language-server analysis.

mod common;

use common::LspTestClient;
use serde_json::{Value, json};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn default_backend_indexes_html5_conditionals() -> TestResult {
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(
        "file:///backend-html5.adoc",
        "= Document\n\nifdef::backend-html5[]\n[[html-only]]\n== HTML Only\nendif::[]\n",
    )?;

    let result = client.send_request(
        "textDocument/documentSymbol",
        json!({
            "textDocument": { "uri": "file:///backend-html5.adoc" }
        }),
    )?;
    assert!(
        contains_symbol_named(&result, "HTML Only"),
        "default HTML5 section should be indexed: {result}"
    );

    client.shutdown();
    Ok(())
}

#[test]
fn configured_backend_controls_conditional_document_symbols() -> TestResult {
    let mut client = LspTestClient::new()?;
    client.initialize_with_options(json!({ "backend": "pdf" }))?;
    client.open_document(
        "file:///backend-pdf.adoc",
        "= Document\n\nifdef::backend-pdf,backend-docbook5[]\n[[pdf-or-docbook]]\n== PDF or DocBook\nendif::backend-pdf,backend-docbook5[]\n",
    )?;

    let result = client.send_request(
        "textDocument/documentSymbol",
        json!({
            "textDocument": { "uri": "file:///backend-pdf.adoc" }
        }),
    )?;
    assert!(
        contains_symbol_named(&result, "PDF or DocBook"),
        "configured backend section should be indexed: {result}"
    );

    client.shutdown();
    Ok(())
}

fn contains_symbol_named(value: &Value, expected: &str) -> bool {
    match value {
        Value::Array(values) => values
            .iter()
            .any(|value| contains_symbol_named(value, expected)),
        Value::Object(object) => {
            object.get("name").and_then(Value::as_str) == Some(expected)
                || object
                    .get("children")
                    .is_some_and(|children| contains_symbol_named(children, expected))
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}
