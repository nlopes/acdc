use std::{error::Error, fs};

use serde_json::{Value, json};
use tower_lsp_server::ls_types::{
    CompletionItem, CompletionTextEdit, Position, TextEdit, WorkspaceEdit,
};

use crate::common::{LspTestClient, Project};

type TestResult = Result<(), Box<dyn Error>>;

fn offset(text: &str, position: Position) -> Result<usize, Box<dyn Error>> {
    let mut start = 0;
    for (index, raw) in text.split_inclusive('\n').enumerate() {
        if index == position.line as usize {
            let line = raw
                .strip_suffix("\r\n")
                .or_else(|| raw.strip_suffix('\n'))
                .unwrap_or(raw);
            let mut units = 0;
            for (byte, ch) in line.char_indices() {
                if units == position.character {
                    return Ok(start + byte);
                }
                units += u32::try_from(ch.len_utf16())?;
            }
            return (units == position.character)
                .then_some(start + line.len())
                .ok_or_else(|| "invalid UTF-16 column".into());
        }
        start += raw.len();
    }
    if position.character == 0
        && position.line as usize == text.bytes().filter(|&b| b == b'\n').count()
        && (text.is_empty() || text.ends_with('\n'))
    {
        return Ok(text.len());
    }
    Err("invalid line".into())
}

fn apply(text: &str, edits: &[TextEdit]) -> Result<String, Box<dyn Error>> {
    let mut edits = edits
        .iter()
        .map(|edit| {
            Ok((
                offset(text, edit.range.start)?,
                offset(text, edit.range.end)?,
                &edit.new_text,
            ))
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    edits.sort_by_key(|&(start, end, _)| (start, end));
    let mut previous_end = 0;
    for &(start, end, _) in &edits {
        assert!(
            previous_end <= start && start <= end,
            "overlapping or reversed edits: {edits:?}"
        );
        previous_end = end;
    }
    let mut output = text.to_owned();
    for (start, end, replacement) in edits.into_iter().rev() {
        output.replace_range(start..end, replacement);
    }
    Ok(output)
}

#[test]
fn formatting_and_wrapping_apply_exactly_to_unicode_crlf_source() -> TestResult {
    let project = Project::new("utf16-source-edits")?;
    let uri = project.uri("document.adoc")?;
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    let text = "é😀  \r\n\r\n\r\n";
    client.open_document(&uri, text)?;
    let edits: Vec<TextEdit> = serde_json::from_value(client.send_request(
        "textDocument/formatting",
        json!({"textDocument":{"uri":uri},"options":{"tabSize":4,"insertSpaces":true}}),
    )?)?;
    assert_eq!(apply(text, &edits)?, "é😀\r\n");
    let invalid = client.send_request("textDocument/rangeFormatting", json!({"textDocument":{"uri":uri},"range":{"start":{"line":0,"character":2},"end":{"line":0,"character":5}},"options":{"tabSize":4,"insertSpaces":true}}))?;
    assert_eq!(invalid, Value::Null);
    let wrap_uri = project.uri("wrap.adoc")?;
    let selected = "é😀 text\r\nnext";
    client.open_document(&wrap_uri, selected)?;
    let actions = client.send_request("textDocument/codeAction", json!({"textDocument":{"uri":wrap_uri},"range":{"start":{"line":0,"character":0},"end":{"line":1,"character":4}},"context":{"diagnostics":[]}}))?;
    let action = actions
        .as_array()
        .ok_or("action array")?
        .iter()
        .find(|action| action.get("title") == Some(&json!("Wrap in listing block")))
        .ok_or("wrap action")?;
    let edit: WorkspaceEdit =
        serde_json::from_value(action.get("edit").ok_or("wrap edit")?.clone())?;
    let changes = edit.changes.ok_or("wrap changes")?;
    assert_eq!(
        apply(
            selected,
            changes.get(&wrap_uri.parse()?).ok_or("wrap edits")?
        )?,
        "----\r\né😀 text\r\nnext\r\n----\r\n"
    );
    let enter_uri = project.uri("enter.adoc")?;
    client.open_document(&enter_uri, "----\r\n")?;
    let edits: Vec<TextEdit> = serde_json::from_value(client.send_request("textDocument/onTypeFormatting", json!({"textDocument":{"uri":enter_uri},"position":{"line":1,"character":0},"ch":"\n","options":{"tabSize":4,"insertSpaces":true}}))?)?;
    assert_eq!(apply("----\r\n", &edits)?, "----\r\n\r\n----\r\n");
    client.shutdown();
    Ok(())
}

#[test]
fn completion_and_signature_help_stop_at_utf16_cursor() -> TestResult {
    let project = Project::new("utf16-completion-edits")?;
    fs::write(project.path().join("😀é.adoc"), "target")?;
    let uri = project.uri("document.adoc")?;
    let text = "😀 {auth}\r\ninclude::😀é\r\n😀 image:photo.png[alt,300]\r\n";
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&uri, text)?;
    let items: Vec<CompletionItem> = serde_json::from_value(client.send_request(
        "textDocument/completion",
        json!({"textDocument":{"uri":uri},"position":{"line":0,"character":8}}),
    )?)?;
    assert!(items.iter().any(|item| item.label == "author"));
    let items: Vec<CompletionItem> = serde_json::from_value(client.send_request(
        "textDocument/completion",
        json!({"textDocument":{"uri":uri},"position":{"line":1,"character":12}}),
    )?)?;
    let edit = items
        .iter()
        .find(|item| item.label == "😀é.adoc")
        .and_then(|item| item.text_edit.as_ref())
        .ok_or("include completion edit")?;
    let CompletionTextEdit::Edit(edit) = edit else {
        return Err("unexpected insert-replace edit".into());
    };
    assert_eq!(edit.range.start, Position::new(1, 9));
    assert_eq!(edit.range.end, Position::new(1, 12));
    assert_eq!(
        apply(text, std::slice::from_ref(edit))?,
        text.replace("include::😀é", "include::😀é.adoc[]")
    );
    let help = client.send_request(
        "textDocument/signatureHelp",
        json!({"textDocument":{"uri":uri},"position":{"line":2,"character":26}}),
    )?;
    assert_eq!(help.get("activeParameter"), Some(&json!(1)));
    for method in ["textDocument/completion", "textDocument/signatureHelp"] {
        let response = client.send_request(
            method,
            json!({"textDocument":{"uri":uri},"position":{"line":0,"character":1}}),
        )?;
        assert_eq!(response, Value::Null);
    }
    client.shutdown();
    Ok(())
}

#[test]
fn file_rename_edits_apply_to_open_closed_and_reindented_include_sources() -> TestResult {
    let project = Project::new("utf16-file-rename-edits")?;
    let book = project.uri("book.adoc")?;
    let child = project.uri("child.adoc")?;
    let closed = project.uri("closed.adoc")?;
    let old = project.uri("😀é.adoc")?;
    let new = project.uri("renamed.adoc")?;
    let text =
        "= Book\r\n\r\ninclude::child.adoc[lines=3..5,indent=0]\r\n\r\ninclude::😀é.adoc[]\r\n";
    let child_text =
        "// omitted\r\n// omitted\r\n    😀 xref:😀é.adoc#target[Go]\r\n\r\n    body\r\n";
    let closed_text = "😀 xref:😀é.adoc#target[Closed]\r\n\r\ninclude::😀é.adoc[]\r\n";
    fs::write(project.path().join("child.adoc"), "Old disk contents")?;
    fs::write(project.path().join("closed.adoc"), closed_text)?;
    fs::write(project.path().join("😀é.adoc"), "[[target]]Here.")?;
    let mut client = LspTestClient::new()?;
    client.initialize_with_params(json!({"processId":null,"capabilities":{},"workspaceFolders":[{"uri":project.uri("")?,"name":"test"}]}))?;
    client.open_document(&child, child_text)?;
    client.open_document(&book, text)?;
    let edit: WorkspaceEdit = serde_json::from_value(client.send_request(
        "workspace/willRenameFiles",
        json!({"files":[{"oldUri":old,"newUri":new}]}),
    )?)?;
    let changes = edit.changes.ok_or("rename changes")?;
    for (uri, original) in [(book, text), (child, child_text), (closed, closed_text)] {
        let edits = changes.get(&uri.parse()?).ok_or("file edits")?;
        assert_eq!(
            apply(original, edits)?,
            original.replace("😀é.adoc", "renamed.adoc")
        );
    }
    client.shutdown();
    Ok(())
}
