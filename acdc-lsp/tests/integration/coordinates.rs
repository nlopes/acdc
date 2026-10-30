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

fn json_field<'a>(value: &'a Value, pointer: &str) -> Result<&'a Value, Box<dyn Error>> {
    value
        .pointer(pointer)
        .ok_or_else(|| format!("Missing {pointer} in {value}").into())
}

#[test]
fn diagnostics_and_symbols_use_original_utf16_ranges() -> TestResult {
    let project = Project::new("utf16-diagnostics-symbols")?;
    let uri = project.uri("document.adoc")?;
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(
        &uri,
        "= 😀 Book\r\n\r\n== 😀 Heading\r\n\r\n😀 See <<missing>>.\r\n",
    )?;
    let diagnostics = client.wait_for_diagnostics(&uri)?;
    let warnings = diagnostics
        .get("diagnostics")
        .and_then(Value::as_array)
        .ok_or("diagnostic array")?;
    let xref = warnings
        .iter()
        .find(|warning| {
            warning
                .get("message")
                .and_then(Value::as_str)
                .is_some_and(|message| message.contains("Unresolved cross-reference"))
        })
        .ok_or("missing xref warning")?;
    assert_eq!(
        json_field(xref, "/range")?,
        &json!({"start":{"line":4,"character":7},"end":{"line":4,"character":18}})
    );
    let outline = client.send_request(
        "textDocument/documentSymbol",
        json!({"textDocument":{"uri":uri}}),
    )?;
    let outline = outline.as_array().ok_or("outline array")?;
    let heading = outline
        .iter()
        .find(|symbol| symbol.get("name").and_then(Value::as_str) == Some("😀 Heading"))
        .ok_or("heading symbol")?;
    let children = heading
        .get("children")
        .and_then(Value::as_array)
        .ok_or("heading children")?;
    let paragraph = children.first().ok_or("paragraph symbol")?;
    assert_eq!(
        json_field(paragraph, "/range/start")?,
        &json!({"line":4,"character":0})
    );
    assert_eq!(
        json_field(paragraph, "/range/end")?,
        &json!({"line":4,"character":19})
    );
    let symbols = client.send_request("workspace/symbol", json!({"query":"😀 Heading"}))?;
    let symbol = symbols
        .as_array()
        .and_then(|symbols| symbols.first())
        .ok_or("workspace symbol")?;
    assert_eq!(
        json_field(symbol, "/location")?,
        &json!({"uri":uri,"range":{"start":{"line":2,"character":0},"end":{"line":2,"character":13}}})
    );
    client.shutdown();
    Ok(())
}

#[test]
fn included_diagnostics_and_symbols_use_selected_reindented_source() -> TestResult {
    let project = Project::new("utf16-included-diagnostics-symbols")?;
    let book = project.uri("book.adoc")?;
    let chapter = project.uri("chapter.adoc")?;
    fs::write(
        project.path().join("chapter.adoc"),
        "// omitted\r\n// omitted\r\n    == 😀 Included\r\n\r\n    😀 See <<missing>>.\r\n",
    )?;
    fs::write(
        project.path().join("middle.adoc"),
        "include::chapter.adoc[lines=3..5,indent=0]\n",
    )?;
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&book, "= Book\n\ninclude::middle.adoc[]\n")?;
    let diagnostics = client.wait_for_diagnostics(&chapter)?;
    let warnings = diagnostics
        .get("diagnostics")
        .and_then(Value::as_array)
        .ok_or("diagnostic array")?;
    let xref = warnings
        .iter()
        .find(|warning| {
            warning
                .get("message")
                .and_then(Value::as_str)
                .is_some_and(|message| message.contains("Unresolved cross-reference"))
        })
        .ok_or("missing included warning")?;
    assert_eq!(
        json_field(xref, "/range")?,
        &json!({"start":{"line":4,"character":11},"end":{"line":4,"character":22}})
    );
    let symbols = client.send_request("workspace/symbol", json!({"query":"😀 Included"}))?;
    let symbols = symbols.as_array().ok_or("workspace symbol array")?;
    assert_eq!(symbols.len(), 1);
    assert_eq!(
        json_field(symbols.first().ok_or("included symbol")?, "/location")?,
        &json!({"uri":chapter,"range":{"start":{"line":2,"character":4},"end":{"line":2,"character":18}}})
    );
    client.shutdown();
    Ok(())
}

#[test]
fn call_hierarchy_uses_utf16_for_crlf_unicode_targets_and_file_extent() -> TestResult {
    let project = Project::new("utf16-call-hierarchy")?;
    let book = project.uri("book.adoc")?;
    let chapter = project.uri("😀.adoc")?;
    fs::write(project.path().join("😀.adoc"), "On disk")?;
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&chapter, "😀 unsaved")?;
    client.open_document(&book, "= Book\r\n\r\ninclude::😀.adoc[]\r\n")?;
    let prepared = client.send_request(
        "textDocument/prepareCallHierarchy",
        json!({"textDocument":{"uri":book},"position":{"line":2,"character":11}}),
    )?;
    let target = prepared
        .as_array()
        .and_then(|items| items.first())
        .ok_or("prepared item")?;
    assert_eq!(
        json_field(target, "/range")?,
        &json!({"start":{"line":0,"character":0},"end":{"line":0,"character":10}})
    );
    let incoming = client.send_request("callHierarchy/incomingCalls", json!({"item":target}))?;
    let caller = incoming
        .as_array()
        .and_then(|items| items.first())
        .ok_or("incoming call")?;
    let expected = json!([{"start":{"line":2,"character":9},"end":{"line":2,"character":16}}]);
    assert_eq!(json_field(caller, "/fromRanges")?, &expected);
    let outgoing = client.send_request(
        "callHierarchy/outgoingCalls",
        json!({"item":json_field(caller, "/from")?}),
    )?;
    let destination = outgoing
        .as_array()
        .and_then(|items| items.first())
        .ok_or("outgoing call")?;
    assert_eq!(json_field(destination, "/fromRanges")?, &expected);
    assert_eq!(json_field(destination, "/to/uri")?, &json!(chapter));
    assert_eq!(
        json_field(destination, "/to/range")?,
        json_field(target, "/range")?
    );
    client.shutdown();
    Ok(())
}

#[test]
fn closed_file_symbols_and_callers_use_disk_text_and_skip_removed_sources() -> TestResult {
    let project = Project::new("utf16-closed-symbols-callers")?;
    let root = project.uri("")?;
    let book = project.uri("book.adoc")?;
    let chapter = project.uri("😀.adoc")?;
    fs::write(
        project.path().join("book.adoc"),
        "= Disk\r\n\r\ninclude::😀.adoc[]\r\n",
    )?;
    fs::write(project.path().join("😀.adoc"), "== 😀 Closed\r\n")?;
    let mut client = LspTestClient::new()?;
    client.initialize_with_params(
        json!({"processId":null,"capabilities":{},"workspaceFolders":[{"uri":root,"name":"test"}]}),
    )?;
    let symbols = client.send_request("workspace/symbol", json!({"query":"😀 Closed"}))?;
    let symbol = symbols
        .as_array()
        .and_then(|symbols| symbols.first())
        .ok_or("closed workspace symbol")?;
    assert_eq!(
        json_field(symbol, "/location")?,
        &json!({"uri":chapter,"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":12}}})
    );
    client.open_document(&chapter, "== 😀 Closed\r\n")?;
    let prepared = client.send_request(
        "textDocument/prepareCallHierarchy",
        json!({"textDocument":{"uri":chapter},"position":{"line":0,"character":0}}),
    )?;
    let target = prepared
        .as_array()
        .and_then(|items| items.first())
        .ok_or("prepared file")?;
    let incoming = client.send_request("callHierarchy/incomingCalls", json!({"item":target}))?;
    let caller = incoming
        .as_array()
        .and_then(|items| items.first())
        .ok_or("disk caller")?;
    assert_eq!(json_field(caller, "/from/uri")?, &json!(book));
    assert_eq!(
        json_field(caller, "/fromRanges")?,
        &json!([{"start":{"line":2,"character":9},"end":{"line":2,"character":16}}])
    );
    let outgoing = client.send_request(
        "callHierarchy/outgoingCalls",
        json!({"item":json_field(caller, "/from")?}),
    )?;
    assert_eq!(
        json_field(&outgoing, "/0/fromRanges")?,
        json_field(caller, "/fromRanges")?
    );
    client.send_notification(
        "textDocument/didClose",
        json!({"textDocument":{"uri":chapter}}),
    )?;
    // A query orders didClose before removal, so the disk symbol is indexed again.
    let symbols = client.send_request("workspace/symbol", json!({"query":"😀 Closed"}))?;
    assert_eq!(symbols.as_array().ok_or("closed symbol array")?.len(), 1);
    fs::remove_file(project.path().join("😀.adoc"))?;
    assert!(
        client
            .send_request("workspace/symbol", json!({"query":"😀 Closed"}))?
            .is_null()
    );
    client.shutdown();
    Ok(())
}

#[test]
fn annotations_and_document_links_use_utf16_after_emoji() -> TestResult {
    let project = Project::new("utf16-annotations")?;
    let uri = project.uri("document.adoc")?;
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    client.open_document(&uri, "= Doc\r\n:name: Value\r\n\r\n[[target]]\r\n== Target\r\n\r\n😀 [[inline]] text\r\n\r\n😀 <<target>> {name}\r\n\r\n😀 link:next.adoc[Next]\r\n")?;
    for (line, cursor, start, end) in [
        (6, 6, 3, 13),
        (8, 6, 3, 13),
        (8, 17, 14, 20),
        (10, 8, 3, 23),
    ] {
        let hovered = client.send_request(
            "textDocument/hover",
            json!({"textDocument":{"uri":uri},"position":{"line":line,"character":cursor}}),
        )?;
        assert!(!hovered.is_null(), "missing hover at {line}:{cursor}");
        assert_eq!(
            json_field(&hovered, "/range")?,
            &json!({"start":{"line":line,"character":start},"end":{"line":line,"character":end}})
        );
    }
    let request = json!({"textDocument":{"uri":uri},"position":{"line":8,"character":6}});
    let definition = client.send_request("textDocument/definition", request.clone())?;
    assert_eq!(
        definition,
        json!({"uri":uri,"range":{"start":{"line":3,"character":0},"end":{"line":3,"character":10}}})
    );
    let prepared = client.send_request("textDocument/prepareRename", request)?;
    assert_eq!(
        json_field(&prepared, "/range")?,
        &json!({"start":{"line":8,"character":5},"end":{"line":8,"character":11}})
    );
    let references = client.send_request("textDocument/references", json!({"textDocument":{"uri":uri},"position":{"line":8,"character":6},"context":{"includeDeclaration":false}}))?;
    assert_eq!(
        references,
        json!([{"uri":uri,"range":{"start":{"line":8,"character":3},"end":{"line":8,"character":13}}}])
    );
    assert!(
        client
            .send_request(
                "textDocument/hover",
                json!({"textDocument":{"uri":uri},"position":{"line":6,"character":1}})
            )?
            .is_null()
    );
    let lenses =
        client.send_request("textDocument/codeLens", json!({"textDocument":{"uri":uri}}))?;
    let lens = lenses
        .as_array()
        .ok_or("lens array")?
        .iter()
        .find(|lens| lens.pointer("/range/start/line") == Some(&json!(6)))
        .ok_or("inline anchor lens")?;
    assert_eq!(
        json_field(lens, "/range")?,
        &json!({"start":{"line":6,"character":3},"end":{"line":6,"character":3}})
    );
    assert_eq!(
        json_field(lens, "/command/arguments/1")?,
        &json!({"line":6,"character":3})
    );
    let hints = client.send_request("textDocument/inlayHint", json!({"textDocument":{"uri":uri},"range":{"start":{"line":8,"character":0},"end":{"line":8,"character":20}}}))?;
    let hints = hints.as_array().ok_or("hint array")?;
    assert_eq!(hints.len(), 2);
    assert!(hints.iter().any(|hint| hint.get("position")
        == Some(&json!({"line":8,"character":13}))
        && hint.get("label") == Some(&json!("→ Target"))));
    assert!(hints.iter().any(|hint| hint.get("position")
        == Some(&json!({"line":8,"character":20}))
        && hint.get("label") == Some(&json!("= Value"))));
    let links = client.send_request(
        "textDocument/documentLink",
        json!({"textDocument":{"uri":uri}}),
    )?;
    let link = links
        .as_array()
        .and_then(|links| links.first())
        .ok_or("document link")?;
    assert_eq!(
        json_field(link, "/range")?,
        &json!({"start":{"line":10,"character":3},"end":{"line":10,"character":23}})
    );
    assert_eq!(
        json_field(link, "/target")?,
        &json!(project.uri("next.adoc")?)
    );
    client.shutdown();
    Ok(())
}
