use std::{cell::RefCell, error::Error, fs, path::PathBuf, rc::Rc};

use serde_json::{Value, json};
use tower_lsp_server::ls_types::Uri;

use crate::common::LspTestClient;

type TestResult = Result<(), Box<dyn Error>>;

struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Result<Self, Box<dyn Error>> {
        let path =
            std::env::temp_dir().join(format!("acdc-lsp-protocol-{name}-{}", std::process::id()));
        fs::create_dir(&path)?;
        // Editors use drive paths. Windows canonicalization adds a verbatim prefix.
        #[cfg(windows)]
        return Ok(Self(path));
        #[cfg(not(windows))]
        Ok(Self(path.canonicalize()?))
    }

    fn uri(&self, name: &str) -> Result<String, Box<dyn Error>> {
        Uri::from_file_path(self.0.join(name))
            .map(|uri| uri.as_str().to_owned())
            .ok_or_else(|| "invalid file URI".into())
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn definition(client: &mut LspTestClient, uri: &str) -> Result<Value, Box<dyn Error>> {
    Ok(client.send_request(
        "textDocument/definition",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 8 }
        }),
    )?)
}

#[test]
fn unsaved_include_changes_rename_and_close_refresh_navigation_and_diagnostics() -> TestResult {
    let project = Project::new("buffers")?;
    let book = project.uri("book.adoc")?;
    let child = project.uri("child.adoc")?;
    fs::write(project.0.join("child.adoc"), "[[disk]]\n== Disk\n")?;
    let mut client = LspTestClient::new()?;
    let initialized = client.initialize()?;
    assert!(
        initialized
            .pointer("/capabilities/workspace/fileOperations/didRename")
            .is_some()
    );
    client.open_document(&book, "See <<buffer>>.\n\ninclude::child.adoc[]\n")?;
    client.wait_for_diagnostics(&book)?;
    assert!(definition(&mut client, &book)?.is_null());

    client.open_document(&child, "// unsaved\n[[buffer]]\n== Buffer\n")?;
    let diagnostics = client.wait_for_diagnostics(&book)?;
    assert_eq!(diagnostics.get("version"), Some(&json!(1)));
    assert_eq!(diagnostics.get("diagnostics"), Some(&json!([])));
    let target = definition(&mut client, &book)?;
    assert_eq!(
        target.get("uri").and_then(Value::as_str),
        Some(child.as_str())
    );
    assert_eq!(target.pointer("/range/start/line"), Some(&json!(1)));

    client.send_notification(
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": child, "version": 2 },
            "contentChanges": [{ "text": "// first\n// second\n[[buffer]]\n== Changed\n" }]
        }),
    )?;
    client.wait_for_diagnostics(&book)?;
    assert_eq!(
        definition(&mut client, &book)?.pointer("/range/start/line"),
        Some(&json!(2))
    );

    let renamed = project.uri("renamed.adoc")?;
    client.send_notification(
        "workspace/didRenameFiles",
        json!({
            "files": [{ "oldUri": child, "newUri": renamed }]
        }),
    )?;
    let diagnostics = client.wait_for_diagnostics(&book)?;
    assert!(
        diagnostics
            .get("diagnostics")
            .and_then(Value::as_array)
            .is_some_and(|items| !items.is_empty())
    );
    assert_eq!(
        definition(&mut client, &book)?
            .get("uri")
            .and_then(Value::as_str),
        Some(renamed.as_str())
    );
    client.send_notification(
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": book, "version": 2 },
            "contentChanges": [{ "text": "See <<buffer>>.\n\ninclude::renamed.adoc[]\n" }]
        }),
    )?;
    client.wait_for_diagnostics(&book)?;
    assert_eq!(
        definition(&mut client, &book)?
            .get("uri")
            .and_then(Value::as_str),
        Some(renamed.as_str())
    );

    client.send_notification(
        "textDocument/didClose",
        json!({ "textDocument": { "uri": renamed } }),
    )?;
    let diagnostics = client.wait_for_diagnostics(&book)?;
    assert!(
        diagnostics
            .get("diagnostics")
            .and_then(Value::as_array)
            .is_some_and(|items| !items.is_empty())
    );
    assert!(definition(&mut client, &book)?.is_null());
    client.shutdown();
    Ok(())
}

#[test]
fn watched_file_events_refresh_nested_includes_and_request_editor_refresh() -> TestResult {
    let project = Project::new("watcher")?;
    fs::write(project.0.join("middle.adoc"), "include::leaf.adoc[]\n")?;
    let book = project.uri("book.adoc")?;
    let leaf = project.uri("leaf.adoc")?;
    let requests = Rc::new(RefCell::new(Vec::<(String, Value)>::new()));
    let captured = Rc::clone(&requests);
    let mut client = LspTestClient::new()?;
    client.set_server_request_handler(move |method, params| {
        captured
            .borrow_mut()
            .push((method.to_owned(), params.clone()));
        Value::Null
    });
    client.initialize_with_params(json!({
        "processId": null,
        "capabilities": { "workspace": {
            "didChangeWatchedFiles": { "dynamicRegistration": true },
            "semanticTokens": { "refreshSupport": true }
        } }
    }))?;
    client.wait_for_server_request("client/registerCapability")?;
    assert!(requests.borrow().iter().any(|(_, params)| {
        params
            .pointer("/registrations/0/method")
            .and_then(Value::as_str)
            == Some("workspace/didChangeWatchedFiles")
    }));
    client.open_document(&book, "See <<created>>.\n\ninclude::middle.adoc[]\n")?;
    client.wait_for_diagnostics(&book)?;
    client.wait_for_server_request("workspace/semanticTokens/refresh")?;

    fs::write(project.0.join("leaf.adoc"), "[[created]]\n== Created\n")?;
    client.send_notification(
        "workspace/didChangeWatchedFiles",
        json!({
            "changes": [{ "uri": leaf, "type": 1 }]
        }),
    )?;
    assert_eq!(
        client.wait_for_diagnostics(&book)?.get("diagnostics"),
        Some(&json!([]))
    );
    client.wait_for_server_request("workspace/semanticTokens/refresh")?;
    assert_eq!(
        definition(&mut client, &book)?
            .get("uri")
            .and_then(Value::as_str),
        Some(leaf.as_str())
    );

    fs::remove_file(project.0.join("leaf.adoc"))?;
    client.send_notification(
        "workspace/didChangeWatchedFiles",
        json!({
            "changes": [{ "uri": leaf, "type": 3 }]
        }),
    )?;
    client.wait_for_diagnostics(&book)?;
    client.wait_for_server_request("workspace/semanticTokens/refresh")?;
    assert!(definition(&mut client, &book)?.is_null());
    client.shutdown();
    Ok(())
}

#[test]
fn queued_include_edits_finish_before_rename_uses_source_positions() -> TestResult {
    let project = Project::new("ordered-edits")?;
    let book = project.uri("book.adoc")?;
    let child = project.uri("child.adoc")?;
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&child, "[[target]]\n== Target\n")?;
    client.open_document(&book, "See <<target>>.\n\ninclude::child.adoc[]\n")?;
    client.wait_for_diagnostics(&book)?;
    for (version, lines) in [(2, 1), (3, 2), (4, 3)] {
        let text = format!("{}[[target]]\n== Target\n", "// leading\n".repeat(lines));
        client.send_notification(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": child, "version": version },
                "contentChanges": [{ "text": text }]
            }),
        )?;
    }
    let result = client.send_request(
        "textDocument/rename",
        json!({
            "textDocument": { "uri": book },
            "position": { "line": 0, "character": 8 },
            "newName": "renamed"
        }),
    )?;
    let line = result
        .get("changes")
        .and_then(|changes| changes.get(child.as_str()))
        .and_then(Value::as_array)
        .and_then(|edits| edits.first())
        .and_then(|edit| edit.pointer("/range/start/line"));
    assert_eq!(line, Some(&json!(3)));
    client.shutdown();
    Ok(())
}
