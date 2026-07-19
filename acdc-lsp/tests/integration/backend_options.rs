//! Integration tests for backend-aware language-server analysis.

use std::cell::Cell;
use std::rc::Rc;

use crate::common::LspTestClient;
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

#[test]
fn resource_scoped_configuration_supports_different_workspace_backends() -> TestResult {
    let mut client = LspTestClient::new()?;
    client.set_server_request_handler(|method, params| {
        if method != "workspace/configuration" {
            return Value::Null;
        }
        params
            .get("items")
            .and_then(Value::as_array)
            .map_or(Value::Null, |items| {
                Value::Array(
                    items
                        .iter()
                        .map(|item| match item.get("scopeUri").and_then(Value::as_str) {
                            Some("file:///workspace/pdf") => json!({ "backend": "pdf" }),
                            _ => json!({ "backend": "html5" }),
                        })
                        .collect(),
                )
            })
    });
    client.initialize_with_params(json!({
        "processId": null,
        "capabilities": {
            "workspace": {
                "configuration": true,
                "workspaceFolders": true
            }
        },
        "rootUri": null,
        "workspaceFolders": [
            { "uri": "file:///workspace/html", "name": "html" },
            { "uri": "file:///workspace/pdf", "name": "pdf" }
        ]
    }))?;

    let content = "= Document\n\nifdef::backend-html5[]\n== HTML Only\nendif::[]\n\nifdef::backend-pdf[]\n== PDF Only\nendif::[]\n";
    client.open_document("file:///workspace/html/index.adoc", content)?;
    client.open_document("file:///workspace/pdf/index.adoc", content)?;

    let html = client.send_request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": "file:///workspace/html/index.adoc" } }),
    )?;
    let pdf = client.send_request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": "file:///workspace/pdf/index.adoc" } }),
    )?;
    assert!(contains_symbol_named(&html, "HTML Only"), "{html}");
    assert!(!contains_symbol_named(&html, "PDF Only"), "{html}");
    assert!(contains_symbol_named(&pdf, "PDF Only"), "{pdf}");
    assert!(!contains_symbol_named(&pdf, "HTML Only"), "{pdf}");

    client.shutdown();
    Ok(())
}

#[test]
fn configuration_change_reparses_open_documents() -> TestResult {
    let backend = Rc::new(Cell::new("html5"));
    let configured_backend = Rc::clone(&backend);
    let mut client = LspTestClient::new()?;
    client.set_server_request_handler(move |method, params| {
        if method != "workspace/configuration" {
            return Value::Null;
        }
        let count = params
            .get("items")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        Value::Array(
            (0..count)
                .map(|_| json!({ "backend": configured_backend.get() }))
                .collect(),
        )
    });
    client.initialize_with_params(json!({
        "processId": null,
        "capabilities": {
            "workspace": {
                "configuration": true,
                "semanticTokens": { "refreshSupport": true },
                "codeLens": { "refreshSupport": true },
                "inlayHint": { "refreshSupport": true }
            }
        },
        "rootUri": null
    }))?;
    client.wait_for_server_request("workspace/semanticTokens/refresh")?;
    let uri = "file:///dynamic-backend.adoc";
    client.open_document(
        uri,
        "= Document\n\nifdef::backend-html5[]\n== HTML Only\nendif::[]\n\nifdef::backend-pdf[]\n== PDF Only\nendif::[]\n",
    )?;

    let before = client.send_request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    )?;
    assert!(contains_symbol_named(&before, "HTML Only"), "{before}");
    assert!(!contains_symbol_named(&before, "PDF Only"), "{before}");

    backend.set("pdf");
    client.send_notification(
        "workspace/didChangeConfiguration",
        json!({ "settings": {} }),
    )?;
    client.wait_for_server_request("workspace/semanticTokens/refresh")?;
    let after = client.send_request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    )?;
    assert!(contains_symbol_named(&after, "PDF Only"), "{after}");
    assert!(!contains_symbol_named(&after, "HTML Only"), "{after}");

    client.shutdown();
    Ok(())
}

#[test]
fn pushed_configuration_updates_and_resets_initialization_fallback() -> TestResult {
    let mut client = LspTestClient::new()?;
    client.initialize_with_params(json!({
        "processId": null,
        "capabilities": {
            "workspace": {
                "semanticTokens": { "refreshSupport": true }
            }
        },
        "rootUri": null,
        "initializationOptions": { "backend": "html5" }
    }))?;
    let uri = "file:///pushed-backend.adoc";
    client.open_document(
        uri,
        "= Document\n\nifdef::backend-html5[]\n== HTML Only\nendif::[]\n\nifdef::backend-pdf[]\n== PDF Only\nendif::[]\n",
    )?;

    client.send_notification(
        "workspace/didChangeConfiguration",
        json!({ "settings": { "acdc-lsp": { "backend": "pdf" } } }),
    )?;
    client.wait_for_server_request("workspace/semanticTokens/refresh")?;
    let pdf = client.send_request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    )?;
    assert!(contains_symbol_named(&pdf, "PDF Only"), "{pdf}");
    assert!(!contains_symbol_named(&pdf, "HTML Only"), "{pdf}");

    client.send_notification(
        "workspace/didChangeConfiguration",
        json!({ "settings": { "backend": null } }),
    )?;
    client.wait_for_server_request("workspace/semanticTokens/refresh")?;
    let reset = client.send_request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    )?;
    assert!(contains_symbol_named(&reset, "HTML Only"), "{reset}");
    assert!(!contains_symbol_named(&reset, "PDF Only"), "{reset}");

    client.shutdown();
    Ok(())
}

#[test]
fn workspace_folder_lifecycle_reapplies_resource_scoped_configuration() -> TestResult {
    let mut client = LspTestClient::new()?;
    client.set_server_request_handler(|method, params| {
        if method != "workspace/configuration" {
            return Value::Null;
        }
        params
            .get("items")
            .and_then(Value::as_array)
            .map_or(Value::Null, |items| {
                Value::Array(
                    items
                        .iter()
                        .map(|item| match item.get("scopeUri").and_then(Value::as_str) {
                            Some("file:///workspace/pdf") => json!({ "backend": "pdf" }),
                            _ => json!({ "backend": "html5" }),
                        })
                        .collect(),
                )
            })
    });
    client.initialize_with_params(json!({
        "processId": null,
        "capabilities": {
            "workspace": {
                "configuration": true,
                "workspaceFolders": true,
                "semanticTokens": { "refreshSupport": true }
            }
        },
        "rootUri": null
    }))?;
    client.wait_for_server_request("workspace/semanticTokens/refresh")?;
    let uri = "file:///workspace/pdf/index.adoc";
    client.open_document(
        uri,
        "= Document\n\nifdef::backend-html5[]\n== HTML Only\nendif::[]\n\nifdef::backend-pdf[]\n== PDF Only\nendif::[]\n",
    )?;

    client.send_notification(
        "workspace/didChangeWorkspaceFolders",
        json!({
            "event": {
                "added": [{ "uri": "file:///workspace/pdf", "name": "pdf" }],
                "removed": []
            }
        }),
    )?;
    client.wait_for_server_request("workspace/semanticTokens/refresh")?;
    let added = client.send_request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    )?;
    assert!(contains_symbol_named(&added, "PDF Only"), "{added}");
    assert!(!contains_symbol_named(&added, "HTML Only"), "{added}");

    client.send_notification(
        "workspace/didChangeWorkspaceFolders",
        json!({
            "event": {
                "added": [],
                "removed": [{ "uri": "file:///workspace/pdf", "name": "pdf" }]
            }
        }),
    )?;
    client.wait_for_server_request("workspace/semanticTokens/refresh")?;
    let removed = client.send_request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    )?;
    assert!(contains_symbol_named(&removed, "HTML Only"), "{removed}");
    assert!(!contains_symbol_named(&removed, "PDF Only"), "{removed}");

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
