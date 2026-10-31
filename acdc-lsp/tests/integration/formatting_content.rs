use std::error::Error;

use serde_json::{Value, json};
use tower_lsp_server::ls_types::TextEdit;

use super::source_edits::apply;
use crate::common::{LspTestClient, Project};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn formatting_preserves_verbatim_source_and_edits_only_requested_lines() -> TestResult {
    let project = Project::new("formatting-content")?;
    let uri = project.uri("document.adoc")?;
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    for (body, protected) in [
        ("----\nkeep  \n----", true),
        ("....\nkeep  \n....", true),
        ("++++\nkeep  \n++++", true),
        ("////\nkeep  \n////", true),
        ("[verse]\n____\nkeep  \n____", true),
        ("[stem]\n++++\nkeep  \n++++", true),
        ("[literal]\nkeep  ", false),
        ("    keep  ", false),
        ("____\nkeep  \n____", false),
        ("keep  ", false),
    ] {
        for ending in ["\n", "\r\n"] {
            let source = format!("= Doc\n\n{body}\n\n😀 after  \n").replace('\n', ending);
            client.open_document(&uri, &source)?;
            let options = json!({"tabSize":4,"insertSpaces":true});
            let edits: Vec<TextEdit> = serde_json::from_value(client.send_request(
                "textDocument/formatting",
                json!({"textDocument":{"uri":uri},"options":options}),
            )?)?;
            let mut expected = source.replace("after  ", "after");
            if !protected {
                expected = expected.replace("keep  ", "keep");
            }
            assert_eq!(apply(&source, &edits)?, expected, "{body:?} {ending:?}");
            let last = u32::try_from(source.lines().count() - 1)?;
            let edits: Vec<TextEdit> = serde_json::from_value(client.send_request(
                "textDocument/rangeFormatting",
                json!({"textDocument":{"uri":uri},"options":options,"range":{"start":{"line":last,"character":0},"end":{"line":last,"character":10}}}),
            )?)?;
            assert_eq!(apply(&source, &edits)?, source.replace("after  ", "after"));
        }
    }
    client.shutdown();
    Ok(())
}

#[test]
fn on_type_preserves_verbatim_content_and_unclosed_source_recovery() -> TestResult {
    let project = Project::new("formatting-on-type-content")?;
    let uri = project.uri("document.adoc")?;
    let mut client = LspTestClient::new()?;
    client.initialize()?;
    for (body, protected) in [
        ("[verse]\n____\n* keep\n\n____", true),
        ("----\n* keep\n\n----", true),
        ("----\n* keep\n", true),
        ("____\n* keep\n\n____", false),
        ("* keep\n", false),
    ] {
        let source = format!("= Doc\n\n{body}\n");
        client.open_document(&uri, &source)?;
        let line = u32::try_from(
            source
                .lines()
                .position(|line| line == "* keep")
                .ok_or("list marker")?
                + 1,
        )?;
        let edits = client.send_request(
            "textDocument/onTypeFormatting",
            json!({"textDocument":{"uri":uri},"position":{"line":line,"character":0},"ch":"\n","options":{"tabSize":4,"insertSpaces":true}}),
        )?;
        if protected {
            assert_eq!(edits, Value::Null, "{body}");
        } else {
            let edits: Vec<TextEdit> = serde_json::from_value(edits)?;
            assert_eq!(edits.len(), 1, "{body}");
            assert_eq!(edits.first().ok_or("continuation")?.new_text, "* ");
        }
    }
    client.shutdown();
    Ok(())
}
