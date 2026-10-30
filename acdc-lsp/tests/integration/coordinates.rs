use std::{error::Error, fs};

use serde_json::{Value, json};

use crate::common::{LspTestClient, Project};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn navigation_uses_utf16_by_default_and_when_other_encodings_are_offered() -> TestResult {
    let project = Project::new("utf16-navigation")?;
    let uri = project.uri("document.adoc")?;
    for capabilities in [
        json!({}),
        json!({"general":{"positionEncodings":["utf-8","utf-16"]}}),
    ] {
        let mut client = LspTestClient::new()?;
        let initialized = client.initialize_with_params(
            json!({"processId":null,"rootUri":null,"capabilities":capabilities}),
        )?;
        assert_eq!(
            initialized.pointer("/capabilities/positionEncoding"),
            Some(&json!("utf-16"))
        );
        client.open_document(&uri, "😀 See [[target]]here.\r\n\r\n😀 See <<target>>.\r\n")?;
        let references = client.send_request("textDocument/references", json!({"textDocument":{"uri":uri},"position":{"line":2,"character":10},"context":{"includeDeclaration":false}}))?;
        assert_eq!(
            references,
            json!([{"uri":uri,"range":{"start":{"line":2,"character":7},"end":{"line":2,"character":17}}}])
        );
        let definition = client.send_request(
            "textDocument/definition",
            json!({"textDocument":{"uri":uri},"position":{"line":2,"character":10}}),
        )?;
        assert_eq!(
            definition,
            json!({"uri":uri,"range":{"start":{"line":0,"character":7},"end":{"line":0,"character":17}}})
        );
        client.shutdown();
    }
    Ok(())
}

#[test]
fn navigation_ranges_use_nested_selected_reindented_original_files() -> TestResult {
    let project = Project::new("utf16-includes")?;
    let book = project.uri("book.adoc")?;
    let chapter = project.uri("chapter.adoc")?;
    fs::write(
        project.path().join("chapter.adoc"),
        "// omitted\r\n// omitted\r\n    😀 [[target]]here.\r\n\r\n    😀 See <<target>>.\r\n",
    )?;
    fs::write(
        project.path().join("middle.adoc"),
        "include::chapter.adoc[lines=3..5,indent=0]\n",
    )?;
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&book, "See <<target>>.\n\ninclude::middle.adoc[]\n")?;
    let definition = client.send_request(
        "textDocument/definition",
        json!({"textDocument":{"uri":book},"position":{"line":0,"character":8}}),
    )?;
    assert_eq!(
        definition,
        json!({"uri":chapter,"range":{"start":{"line":2,"character":7},"end":{"line":2,"character":17}}})
    );
    let references = client.send_request("textDocument/references", json!({"textDocument":{"uri":book},"position":{"line":0,"character":8},"context":{"includeDeclaration":false}}))?;
    let references = references.as_array().ok_or("reference array")?;
    assert_eq!(references.len(), 2);
    assert!(references.contains(&json!({"uri":book,"range":{"start":{"line":0,"character":4},"end":{"line":0,"character":14}}})));
    assert!(references.contains(&json!({"uri":chapter,"range":{"start":{"line":4,"character":11},"end":{"line":4,"character":21}}})));
    client.shutdown();
    Ok(())
}

#[test]
fn navigation_prefers_unsaved_include_text_to_disk() -> TestResult {
    let project = Project::new("utf16-overlay")?;
    let book = project.uri("book.adoc")?;
    let chapter = project.uri("chapter.adoc")?;
    fs::write(project.path().join("chapter.adoc"), "[[old]]On disk.\n")?;
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&chapter, "😀 See [[target]]Unsaved.\n")?;
    client.open_document(&book, "See <<target>>.\n\ninclude::chapter.adoc[]\n")?;
    let definition: Value = client.send_request(
        "textDocument/definition",
        json!({"textDocument":{"uri":book},"position":{"line":0,"character":8}}),
    )?;
    assert_eq!(
        definition,
        json!({"uri":chapter,"range":{"start":{"line":0,"character":7},"end":{"line":0,"character":17}}})
    );
    client.shutdown();
    Ok(())
}
