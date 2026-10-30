use std::error::Error;

use serde_json::{Value, json};
use tower_lsp_server::ls_types::{
    CompletionItem, CompletionTextEdit, Position, Range, SelectionRange, TextEdit, WorkspaceEdit,
};

use crate::common::{LspTestClient, Project};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn incoming_columns_past_line_end_use_canonical_utf16_positions() -> TestResult {
    let project = Project::new("coordinate-audit-clamping")?;
    let uri = project.uri("document.adoc")?;
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&uri, "😀 {auth\r\n\r\né😀 text  \r\n")?;
    let items: Vec<CompletionItem> = serde_json::from_value(client.send_request(
        "textDocument/completion",
        json!({"textDocument":{"uri":uri},"position":{"line":0,"character":999}}),
    )?)?;
    assert!(items.iter().any(|item| item.label == "author"));
    let edits: Vec<TextEdit> = serde_json::from_value(client.send_request("textDocument/rangeFormatting", json!({"textDocument":{"uri":uri},"range":{"start":{"line":2,"character":0},"end":{"line":2,"character":999}},"options":{"tabSize":4,"insertSpaces":true}}))?)?;
    assert!(
        edits
            .iter()
            .any(|edit| edit.range == Range::new(Position::new(2, 8), Position::new(2, 10)))
    );
    let selections: Vec<SelectionRange> = serde_json::from_value(client.send_request(
        "textDocument/selectionRange",
        json!({"textDocument":{"uri":uri},"positions":[{"line":2,"character":999}]}),
    )?)?;
    let canonical: Vec<SelectionRange> = serde_json::from_value(client.send_request(
        "textDocument/selectionRange",
        json!({"textDocument":{"uri":uri},"positions":[{"line":2,"character":10}]}),
    )?)?;
    assert_eq!(selections, canonical);
    let actions = client.send_request("textDocument/codeAction", json!({"textDocument":{"uri":uri},"range":{"start":{"line":2,"character":0},"end":{"line":2,"character":999}},"context":{"diagnostics":[]}}))?;
    let action = actions
        .as_array()
        .ok_or("actions")?
        .iter()
        .find(|action| action.get("title") == Some(&json!("Wrap in listing block")))
        .ok_or("wrap")?;
    let edit: WorkspaceEdit = serde_json::from_value(action.get("edit").ok_or("edit")?.clone())?;
    let edits = edit.changes.ok_or("changes")?;
    assert_eq!(
        edits
            .get(&uri.parse()?)
            .and_then(|edits| edits.first())
            .ok_or("wrap edit")?
            .range,
        Range::new(Position::new(2, 0), Position::new(2, 10))
    );
    let invalid = client.send_request(
        "textDocument/completion",
        json!({"textDocument":{"uri":uri},"position":{"line":0,"character":1}}),
    )?;
    assert_eq!(invalid, Value::Null);
    client.shutdown();
    Ok(())
}

#[test]
fn full_sync_uses_last_snapshot_and_never_replaces_source_with_a_fragment() -> TestResult {
    let project = Project::new("coordinate-audit-sync")?;
    let uri = project.uri("document.adoc")?;
    let mut client = LspTestClient::new()?;
    let initialized = client.initialize()?;
    assert_eq!(
        initialized.pointer("/capabilities/textDocumentSync/change"),
        Some(&json!(1))
    );
    client.open_document(&uri, "Initial")?;
    client.send_notification("textDocument/didChange", json!({"textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":"Intermediate"},{"text":"😀 {auth\r\n"}]}))?;
    let request = json!({"textDocument":{"uri":uri},"position":{"line":0,"character":8}});
    let items: Vec<CompletionItem> =
        serde_json::from_value(client.send_request("textDocument/completion", request.clone())?)?;
    assert!(items.iter().any(|item| item.label == "author"));
    client.send_notification("textDocument/didChange", json!({"textDocument":{"uri":uri,"version":3},"contentChanges":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":2}},"text":"Fragment"}]}))?;
    let items: Vec<CompletionItem> =
        serde_json::from_value(client.send_request("textDocument/completion", request)?)?;
    assert!(items.iter().any(|item| item.label == "author"));
    client.shutdown();
    Ok(())
}

#[test]
fn completion_edits_use_the_normalized_cursor_after_unicode() -> TestResult {
    let project = Project::new("coordinate-audit-completion")?;
    let uri = project.uri("document.adoc")?;
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&uri, "é😀 ima\r\n")?;
    let items: Vec<CompletionItem> = serde_json::from_value(client.send_request(
        "textDocument/completion",
        json!({"textDocument":{"uri":uri},"position":{"line":0,"character":999}}),
    )?)?;
    let edit = items
        .iter()
        .find_map(|item| match &item.text_edit {
            Some(CompletionTextEdit::Edit(edit)) => Some(edit),
            _ => None,
        })
        .ok_or("snippet edit")?;
    assert_eq!(
        edit.range,
        Range::new(Position::new(0, 4), Position::new(0, 7))
    );
    assert!(edit.new_text.starts_with("image:"));
    client.shutdown();
    Ok(())
}
