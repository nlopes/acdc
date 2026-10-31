use std::{error::Error, fs};

use serde_json::json;
use tower_lsp_server::ls_types::{CompletionItem, CompletionItemKind, Position};

use crate::common::{LspTestClient, Project};

type TestResult = Result<(), Box<dyn Error>>;

fn complete(
    client: &mut LspTestClient,
    uri: &str,
    source: &str,
    line_text: &str,
) -> Result<Vec<CompletionItem>, Box<dyn Error>> {
    let line = source
        .lines()
        .position(|line| line == line_text)
        .ok_or("completion line")?;
    let position = Position::new(
        u32::try_from(line)?,
        u32::try_from(line_text.encode_utf16().count())?,
    );
    Ok(serde_json::from_value(client.send_request(
        "textDocument/completion",
        json!({"textDocument":{"uri":uri}, "position":position}),
    )?)?)
}

#[test]
fn completion_retains_included_attributes_backend_flags_and_owned_targets() -> TestResult {
    let project = Project::new("completion-state")?;
    let book = project.uri("book.adoc")?;
    let chapter = project.uri("chapter.adoc")?;
    fs::write(
        project.path().join("header.adoc"),
        ":custom-included: yes\n:custom-empty:\n:custom-removed:\n:custom-removed!:\n",
    )?;
    let source = "= Document\ninclude::header.adoc[]\n\n:custom-body: value\n\n[[target-local]]\n== Local\n\n😀 {custom-\n{backend-\n{max-include-depth\n<<target-\n";
    for backend in ["html5", "pdf"] {
        let mut client = LspTestClient::new()?;
        client.initialize_with_options(json!({"backend":backend}))?;
        client.open_document(&chapter, "[[target-other]]\n== Other\n")?;
        client.open_document(&book, source)?;
        let mut attributes = complete(&mut client, &book, source, "😀 {custom-")?
            .into_iter()
            .map(|item| item.label)
            .collect::<Vec<_>>();
        attributes.sort_unstable();
        assert_eq!(attributes, ["custom-empty", "custom-included"], "{backend}");
        let flags = complete(&mut client, &book, source, "{backend-")?;
        assert!(
            flags
                .iter()
                .any(|item| item.label == format!("backend-{backend}")
                    && item.kind == Some(CompletionItemKind::VARIABLE)),
            "{backend}"
        );
        let other_backend = if backend == "pdf" { "html5" } else { "pdf" };
        assert!(
            !flags
                .iter()
                .any(|item| item.label == format!("backend-{other_backend}")
                    && item.kind == Some(CompletionItemKind::VARIABLE)),
            "{backend}"
        );
        assert!(
            complete(&mut client, &book, source, "{max-include-depth")?
                .iter()
                .any(|item| item.label == "max-include-depth")
        );
        let targets = complete(&mut client, &book, source, "<<target-")?;
        assert!(
            targets
                .iter()
                .any(|item| item.label == "target-local" && item.insert_text.is_none())
        );
        assert!(targets.iter().any(|item| item.label == "target-other"
            && item.insert_text.as_deref() == Some("chapter.adoc#target-other")));
        client.shutdown();
    }
    Ok(())
}
