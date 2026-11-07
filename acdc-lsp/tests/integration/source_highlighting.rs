//! Source-policy regressions for semantic tokens and selection ranges.

use std::{error::Error, fs};

use serde_json::json;
use tower_lsp_server::ls_types::{Position, Range, SelectionRange, SemanticTokens};

use crate::common::{LspTestClient, Project};

type TestResult = Result<(), Box<dyn Error>>;

fn decoded(client: &mut LspTestClient, uri: &str) -> Result<Vec<[u32; 5]>, Box<dyn Error>> {
    let tokens: SemanticTokens = serde_json::from_value(client.send_request(
        "textDocument/semanticTokens/full",
        json!({"textDocument":{"uri":uri}}),
    )?)?;
    let mut line = 0;
    let mut column = 0;
    Ok(tokens
        .data
        .into_iter()
        .map(|token| {
            line += token.delta_line;
            column = if token.delta_line == 0 {
                column + token.delta_start
            } else {
                token.delta_start
            };
            [
                line,
                column,
                token.length,
                token.token_type,
                token.token_modifiers_bitset,
            ]
        })
        .collect())
}

fn source_range(line: u32, text: &str, spelling: &str) -> Result<Range, Box<dyn Error>> {
    let start = text.find(spelling).ok_or("source spelling")?;
    let column = u32::try_from(text.get(..start).ok_or("prefix")?.encode_utf16().count())?;
    let length = u32::try_from(spelling.encode_utf16().count())?;
    Ok(Range::new(
        Position::new(line, column),
        Position::new(line, column + length),
    ))
}

#[test]
fn source_highlighting_keeps_macro_curved_and_include_policies() -> TestResult {
    let project = Project::new("source-highlight-policies")?;
    let uri = project.uri("document.adoc")?;
    fs::write(
        project.path().join("child.adoc"),
        "// omitted\n// omitted\n    *Foreign*\n",
    )?;
    let body = "😀 image:icon.svg[*Alt*] \"`*curved*`\" *bold*";
    let source = format!(
        "= Source\r\n:rich: *Expanded*\r\n\r\ninclude::child.adoc[lines=3..3,indent=0]\r\n\r\n{body}\r\n{{rich}}\r\n"
    );
    for mode in ["asciidoctor", "draft"] {
        let mut client = LspTestClient::new()?;
        client.initialize_with_params(json!({"processId":null,"capabilities":{},"initializationOptions":{"compatibilityMode":mode}}))?;
        client.open_document(&uri, &source)?;
        let tokens = decoded(&mut client, &uri)?;
        let image = source_range(5, body, "image:icon.svg[*Alt*]")?;
        let bold = source_range(5, body, "*bold*")?;
        assert!(
            tokens.contains(&[
                5,
                image.start.character,
                image.end.character - image.start.character,
                1,
                0
            ]),
            "{mode}: {tokens:?}"
        );
        assert!(
            tokens.contains(&[
                5,
                bold.start.character,
                bold.end.character - bold.start.character,
                4,
                0
            ]),
            "{mode}: {tokens:?}"
        );
        assert_eq!(
            tokens.iter().filter(|token| token[0] == 5).count(),
            2,
            "{mode}: {tokens:?}"
        );
        assert!(
            tokens.iter().all(|token| matches!(token[0], 1 | 5 | 6)),
            "{mode}: {tokens:?}"
        );
        let alt = source_range(5, body, "Alt")?;
        let selections: Vec<SelectionRange> = serde_json::from_value(client.send_request(
            "textDocument/selectionRange",
            json!({"textDocument":{"uri":uri},"positions":[alt.start]}),
        )?)?;
        assert_eq!(selections.first().ok_or("macro selection")?.range, image);
        for token in tokens {
            let text = source.lines().nth(token[0] as usize).ok_or("token line")?;
            assert!(
                token[2] > 0
                    && usize::try_from(token[1] + token[2])? <= text.encode_utf16().count()
            );
        }
        client.shutdown();
    }
    Ok(())
}

#[test]
fn authored_section_ids_share_definition_highlighting() -> TestResult {
    let project = Project::new("source-highlight-ids")?;
    let uri = project.uri("document.adoc")?;
    for mode in ["asciidoctor", "draft"] {
        let mut client = LspTestClient::new()?;
        client.initialize_with_params(json!({"processId":null,"capabilities":{},"initializationOptions":{"compatibilityMode":mode}}))?;
        client.open_document(
            &uri,
            "= Targets\n\n[#named]\n== Named\n\n[[anchor]]\n== Anchored\n\n== Generated\n",
        )?;
        let sections = decoded(&mut client, &uri)?
            .into_iter()
            .filter(|token| token[3] == 0)
            .map(|token| (token[0], token[4]))
            .collect::<Vec<_>>();
        assert_eq!(sections, [(3, 2), (6, 2), (8, 0)], "{mode}");
        client.shutdown();
    }
    Ok(())
}

#[test]
fn conditional_overlays_follow_body_changes_and_selected_nested_includes() -> TestResult {
    let project = Project::new("conditional-source-order")?;
    let uri = project.uri("main.adoc")?;
    fs::write(
        project.path().join("selected.adoc"),
        "// tag::chosen[]\ninclude::nested.adoc[lines=2]\n// end::chosen[]\n:omitted:\n",
    )?;
    fs::write(project.path().join("nested.adoc"), ":unused:\n:included:\n")?;
    let source = concat!(
        "= Conditional overlays\r\n:header: set\r\n\r\n:body:\r\n",
        "ifdef::body[]\r\n== Body selected é😀\r\nendif::[]\r\n\r\n",
        ":header!:\r\nifdef::header[]\r\n== Header excluded\r\nendif::[]\r\n\r\n",
        "include::selected.adoc[tag=chosen]\r\n",
        "ifdef::included[]\r\n== Include selected\r\nendif::[]\r\n\r\n",
        "ifdef::omitted[]\r\n== Include excluded\r\nendif::[]\r\n\r\n",
        "ifdef::missing[]\r\n:leak:\r\nendif::[]\r\n",
        "ifdef::leak[]\r\n== Leak excluded\r\nendif::[]\r\n\r\n",
        "ifeval::[1 == 0]\r\nifdef::body[]\r\n== Nested excluded\r\nendif::[]\r\nendif::[]\r\n\r\n",
        "ifdef::backend-html5[]\r\n== HTML selected\r\nendif::[]\r\n",
        "ifdef::backend-pdf[]\r\n== PDF selected\r\nendif::[]\r\n\r\n",
        "Outside *bold* é😀.\r\n",
    );
    for mode in ["asciidoctor", "draft"] {
        for backend in ["html5", "pdf"] {
            let mut client = LspTestClient::new()?;
            client.initialize_with_options(json!({"compatibilityMode":mode,"backend":backend}))?;
            client.open_document(&uri, source)?;
            let diagnostics = client.wait_for_diagnostics(&uri)?;
            let tokens = decoded(&mut client, &uri)?;
            let symbols = client.send_request(
                "textDocument/documentSymbol",
                json!({"textDocument":{"uri":uri}}),
            )?;
            let names = serde_json::to_string(&symbols)?;
            for (line, text) in source.lines().enumerate() {
                let line = u32::try_from(line)?;
                if text.starts_with("== ") {
                    let inactive = text.contains("excluded")
                        || (text.contains("HTML selected") && backend == "pdf")
                        || (text.contains("PDF selected") && backend == "html5");
                    assert_eq!(
                        tokens.contains(&[
                            line,
                            0,
                            u32::try_from(text.encode_utf16().count())?,
                            5,
                            4
                        ]),
                        inactive,
                        "{mode}/{backend}: {text}: {tokens:?}"
                    );
                    assert_eq!(
                        names.contains(text.trim_start_matches("== ")),
                        !inactive,
                        "{mode}/{backend}: {names}"
                    );
                    let hints = diagnostics
                        .get("diagnostics")
                        .and_then(serde_json::Value::as_array)
                        .ok_or("diagnostics")?;
                    assert_eq!(
                        hints.iter().any(|hint| hint
                            .get("message")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|message| message.starts_with("Inactive conditional"))
                            && hint.pointer("/range/start/line") == Some(&json!(line - 1))),
                        inactive
                    );
                } else if text.starts_with("Outside") {
                    let bold = source_range(line, text, "*bold*")?;
                    assert!(tokens.contains(&[
                        line,
                        bold.start.character,
                        bold.end.character - bold.start.character,
                        4,
                        0
                    ]));
                    assert_eq!(tokens.iter().filter(|token| token[0] == line).count(), 1);
                }
            }
            client.shutdown();
        }
    }
    Ok(())
}
