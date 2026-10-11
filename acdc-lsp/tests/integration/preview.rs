use crate::common::LspTestClient;
use serde_json::{Value, json};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const COMMAND: &str = "acdc.previewHtml";
const CONFIGURED_COMMAND: &str = "acdc.preview";

fn actions(
    client: &mut LspTestClient,
    uri: &str,
    only: &Value,
) -> Result<Value, crate::common::HarnessError> {
    client.send_request("textDocument/codeAction", json!({
        "textDocument": { "uri": uri },
        "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } },
        "context": { "diagnostics": [], "only": only }
    }))
}

fn has_preview(actions: &Value) -> bool {
    actions.as_array().is_some_and(|actions| {
        actions.iter().any(|action| {
            action
                .get("command")
                .and_then(|command| command.get("command"))
                .and_then(Value::as_str)
                .is_some_and(|command| matches!(command, COMMAND | CONFIGURED_COMMAND))
        })
    })
}

#[test]
fn preview_is_not_offered_without_client_support() -> TestResult {
    let mut client = LspTestClient::new()?;
    let result = client.initialize()?;
    assert!(
        result
            .pointer("/capabilities/executeCommandProvider")
            .is_none()
    );
    let uri = "untitled:preview.adoc";
    client.open_document(uri, "= Preview\n\nText.\n")?;
    assert!(!has_preview(&actions(
        &mut client,
        uri,
        &json!(["source"])
    )?));
    assert!(
        client
            .send_request(
                "workspace/executeCommand",
                json!({
                    "command": COMMAND, "arguments": [uri]
                })
            )
            .is_err()
    );
    client.shutdown();
    Ok(())
}

#[cfg(not(feature = "preview"))]
#[test]
fn preview_feature_off_ignores_client_support() -> TestResult {
    let mut client = LspTestClient::new()?;
    let result = client.initialize_with_params(json!({
        "capabilities": { "window": { "showDocument": { "support": true } } }
    }))?;
    assert!(
        result
            .pointer("/capabilities/executeCommandProvider")
            .is_none()
    );
    client.open_document("untitled:preview.adoc", "= Preview\n")?;
    assert!(!has_preview(&actions(
        &mut client,
        "untitled:preview.adoc",
        &json!(["source"])
    )?));
    client.shutdown();
    Ok(())
}

#[cfg(feature = "preview")]
mod enabled {
    use std::{cell::RefCell, fs, rc::Rc};

    use tower_lsp_server::ls_types::Uri;

    use super::{COMMAND, CONFIGURED_COMMAND, TestResult, actions, has_preview};
    use crate::common::{HarnessError, LspTestClient, Project};
    use serde_json::{Value, json};

    fn client_with_preview(
        success: bool,
        requests: Rc<RefCell<Vec<Value>>>,
    ) -> Result<LspTestClient, HarnessError> {
        let mut client = LspTestClient::new()?;
        client.set_server_request_handler(move |method, params| {
            if method == "window/showDocument" {
                requests.borrow_mut().push(params.clone());
                return json!({ "success": success });
            }
            Value::Null
        });
        let result = client.initialize_with_params(json!({
            "capabilities": { "window": { "showDocument": { "support": true } } },
            "initializationOptions": { "backend": "pdf" }
        }))?;
        assert_eq!(
            result.pointer("/capabilities/executeCommandProvider/commands"),
            Some(&json!([CONFIGURED_COMMAND, COMMAND]))
        );
        Ok(client)
    }

    fn execute(client: &mut LspTestClient, arguments: &Value) -> Result<Value, HarnessError> {
        execute_command(client, COMMAND, arguments)
    }

    fn execute_command(
        client: &mut LspTestClient,
        command: &str,
        arguments: &Value,
    ) -> Result<Value, HarnessError> {
        client.send_request(
            "workspace/executeCommand",
            json!({
                "command": command, "arguments": arguments
            }),
        )
    }

    fn preview_titles(actions: &Value) -> Vec<&str> {
        actions
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|action| {
                action
                    .get("title")
                    .and_then(Value::as_str)
                    .filter(|title| title.starts_with("Preview AsciiDoc"))
            })
            .collect()
    }

    #[cfg(feature = "preview-pdf")]
    #[test]
    fn pdf_preview_renders_unsaved_pdf_content_and_confines_relative_images() -> TestResult {
        let project = Project::new("preview-pdf")?;
        let outside = Project::new("preview-pdf-outside")?;
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="30" height="30"><rect width="30" height="30" fill="green"/></svg>"#;
        fs::write(project.path().join("local.svg"), svg)?;
        fs::write(outside.path().join("outside.svg"), svg)?;
        fs::write(project.path().join("part.adoc"), "DISKINCLUDE\n")?;
        fs::write(project.path().join("doc.adoc"), "= DISKROOT\n")?;
        let uri = project.uri("doc.adoc")?;
        let source = format!(
            "= Preview\n:pdf-page-layout: landscape\n\nifdef::backend-pdf[]\n== PDFONLY\n\nUNSAVEDPDF\nendif::[]\n\nifdef::backend-html5[]\n== HTMLONLY\nendif::[]\n\ninclude::part.adoc[]\n\nimage::local.svg[LOCALIMAGEALT]\n\nimage::{}[OUTSIDEIMAGEALT]\n",
            outside.path().join("outside.svg").display(),
        );
        let requests = Rc::new(RefCell::new(Vec::new()));
        let mut client = client_with_preview(true, Rc::clone(&requests))?;
        client.open_document(&project.uri("part.adoc")?, "EDITORINCLUDE\n")?;
        client.open_document(&uri, &source)?;
        assert_eq!(
            preview_titles(&actions(&mut client, &uri, &json!(["source"]))?),
            ["Preview AsciiDoc as PDF", "Preview AsciiDoc as HTML"]
        );
        execute_command(&mut client, CONFIGURED_COMMAND, &json!([uri]))?;
        let pdf_uri = preview_uri(&requests)?;
        let pdf_path = pdf_uri.to_file_path().ok_or("expected local PDF preview")?;
        assert_eq!(
            pdf_path.extension().and_then(|ext| ext.to_str()),
            Some("pdf")
        );
        let bytes = fs::read(&pdf_path)?;
        assert!(bytes.starts_with(b"%PDF-"));
        let pdf = lopdf::Document::load_mem(&bytes)?;
        let pages = pdf.get_pages();
        let text = pdf.extract_text(&pages.keys().copied().collect::<Vec<_>>())?;
        for marker in ["PDFONLY", "UNSAVEDPDF", "EDITORINCLUDE", "OUTSIDEIMAGEALT"] {
            assert!(text.contains(marker), "missing {marker}: {text}");
        }
        for marker in ["HTMLONLY", "DISKROOT", "DISKINCLUDE", "LOCALIMAGEALT"] {
            assert!(!text.contains(marker), "unexpected {marker}: {text}");
        }
        let page = pdf
            .get_object(*pages.values().next().ok_or("missing PDF page")?)?
            .as_dict()?;
        let [_, _, width, height] = page.get(b"MediaBox")?.as_array()?.as_slice() else {
            return Err("invalid PDF page dimensions".into());
        };
        assert!(width.as_float()? > height.as_float()?);

        execute(&mut client, &json!([uri]))?;
        let html_uri = preview_uri(&requests)?;
        assert_ne!(html_uri, pdf_uri);
        let html_path = html_uri.to_file_path().ok_or("expected HTML preview")?;
        let html = fs::read_to_string(&html_path)?;
        assert!(html.contains("HTMLONLY"));
        assert!(!html.contains("PDFONLY"));
        assert_eq!(fs::read(&pdf_path)?, bytes);
        client.send_notification(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": 2 },
                "contentChanges": [{ "text": "= Updated\n\nPDFREFRESHED\n" }]
            }),
        )?;
        execute_command(&mut client, CONFIGURED_COMMAND, &json!([uri]))?;
        assert_eq!(preview_uri(&requests)?, pdf_uri);
        let refreshed = lopdf::Document::load(&pdf_path)?;
        assert!(refreshed.extract_text(&[1])?.contains("PDFREFRESHED"));
        assert_eq!(fs::read_to_string(&html_path)?, html);
        assert!(fs::read_to_string(project.path().join("doc.adoc"))?.contains("DISKROOT"));
        client.shutdown();
        assert!(!pdf_path.exists());
        assert!(!html_path.exists());
        Ok(())
    }

    #[test]
    fn configured_preview_follows_backend_changes_and_keeps_html_explicit() -> TestResult {
        let requests = Rc::new(RefCell::new(Vec::new()));
        let mut client = client_with_preview(true, Rc::clone(&requests))?;
        let uri = "untitled:formats.adoc";
        client.open_document(uri, "= Preview\n\nifdef::backend-html5s[]\n== SEMANTIC\nendif::[]\n\nifdef::backend-html5[]\n== STANDARD\nendif::[]\n")?;
        for (backend, title, marker) in [
            ("html5s", "Preview AsciiDoc as semantic HTML", "SEMANTIC"),
            ("html5", "Preview AsciiDoc as HTML", "STANDARD"),
        ] {
            client.send_notification(
                "workspace/didChangeConfiguration",
                json!({"settings": {"backend": backend}}),
            )?;
            let offered = actions(&mut client, uri, &json!(["source"]))?;
            assert_eq!(preview_titles(&offered).first(), Some(&title));
            execute_command(&mut client, CONFIGURED_COMMAND, &json!([uri]))?;
            let html = fs::read_to_string(
                preview_uri(&requests)?
                    .to_file_path()
                    .ok_or("expected local HTML preview")?,
            )?;
            assert!(html.contains(marker));
            if backend == "html5s" {
                assert!(!html.contains("STANDARD"));
            } else {
                assert!(!html.contains("SEMANTIC"));
            }
        }
        client.send_notification(
            "workspace/didChangeConfiguration",
            json!({"settings": {"backend": "manpage"}}),
        )?;
        assert_eq!(
            preview_titles(&actions(&mut client, uri, &json!(["source"]))?),
            ["Preview AsciiDoc as HTML"]
        );
        let count = requests.borrow().len();
        assert!(execute_command(&mut client, CONFIGURED_COMMAND, &json!([uri])).is_err());
        assert_eq!(requests.borrow().len(), count);
        execute(&mut client, &json!([uri]))?;
        client.shutdown();
        Ok(())
    }

    #[cfg(feature = "preview-pdf")]
    #[test]
    fn pdf_preview_supports_untitled_buffers_and_reports_viewer_refusal() -> TestResult {
        let requests = Rc::new(RefCell::new(Vec::new()));
        let mut client = client_with_preview(false, Rc::clone(&requests))?;
        let uri = "untitled:pdf.adoc";
        client.open_document(uri, "= Unsaved PDF\n\nUNTITLEDPDF\n")?;
        let Err(HarnessError::Lsp { error, .. }) =
            execute_command(&mut client, CONFIGURED_COMMAND, &json!([uri]))
        else {
            return Err("expected external viewer failure".into());
        };
        assert_eq!(error.get("code"), Some(&json!(-32803)));
        let pdf = lopdf::Document::load(
            preview_uri(&requests)?
                .to_file_path()
                .ok_or("expected local PDF")?,
        )?;
        assert!(pdf.extract_text(&[1])?.contains("UNTITLEDPDF"));
        client.shutdown();
        Ok(())
    }

    #[cfg(feature = "preview-pdf")]
    #[test]
    fn preview_actions_use_each_workspace_folders_backend() -> TestResult {
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
                            .map(|item| {
                                if item.get("scopeUri").and_then(Value::as_str)
                                    == Some("file:///workspace/pdf")
                                {
                                    json!({"backend": "pdf"})
                                } else {
                                    json!({"backend": "html5"})
                                }
                            })
                            .collect(),
                    )
                })
        });
        client.initialize_with_params(json!({
            "capabilities": {
                "window": {"showDocument": {"support": true}},
                "workspace": {"configuration": true, "workspaceFolders": true}
            },
            "workspaceFolders": [
                {"uri": "file:///workspace/html", "name": "html"},
                {"uri": "file:///workspace/pdf", "name": "pdf"}
            ]
        }))?;
        for (uri, title) in [
            (
                "file:///workspace/html/doc.adoc",
                "Preview AsciiDoc as HTML",
            ),
            ("file:///workspace/pdf/doc.adoc", "Preview AsciiDoc as PDF"),
        ] {
            client.open_document(uri, "= Preview\n")?;
            assert_eq!(
                preview_titles(&actions(&mut client, uri, &json!(["source"]))?).first(),
                Some(&title)
            );
        }
        client.shutdown();
        Ok(())
    }

    #[cfg(not(feature = "preview-pdf"))]
    #[test]
    fn disabled_pdf_preview_keeps_html_and_reports_missing_feature() -> TestResult {
        let requests = Rc::new(RefCell::new(Vec::new()));
        let mut client = client_with_preview(true, Rc::clone(&requests))?;
        let uri = "untitled:preview.adoc";
        client.open_document(uri, "= Preview\n")?;
        assert_eq!(
            preview_titles(&actions(&mut client, uri, &json!(["source"]))?),
            ["Preview AsciiDoc as HTML"]
        );
        let Err(HarnessError::Lsp { error, .. }) =
            execute_command(&mut client, CONFIGURED_COMMAND, &json!([uri]))
        else {
            return Err("expected unavailable PDF preview".into());
        };
        assert_eq!(error.get("code"), Some(&json!(-32803)));
        assert!(
            error
                .get("message")
                .and_then(Value::as_str)
                .is_some_and(|message| message.contains("preview-pdf"))
        );
        assert!(requests.borrow().is_empty());
        execute(&mut client, &json!([uri]))?;
        client.shutdown();
        Ok(())
    }

    fn preview_uri(requests: &RefCell<Vec<Value>>) -> Result<Uri, Box<dyn std::error::Error>> {
        let requests = requests.borrow();
        let request = requests.last().ok_or("no showDocument request")?;
        assert_eq!(request.get("external"), Some(&json!(true)));
        assert_eq!(request.get("takeFocus"), Some(&json!(true)));
        Ok(request
            .get("uri")
            .and_then(Value::as_str)
            .ok_or("missing preview URI")?
            .parse()?)
    }

    #[test]
    fn preview_renders_unsaved_html_without_changing_analysis_and_keeps_links_and_lifetime()
    -> TestResult {
        let project = Project::new("preview-resources")?;
        let name = "source & café.adoc";
        let uri = project.uri(name)?;
        fs::write(project.path().join(name), "= Disk version\n\nDISKONLY\n")?;
        let source = concat!(
            "= Buffer version\n:toc:\n\n",
            "ifdef::backend-html5[]\n== HTMLONLY\n\nUNSAVEDBUFFER\nendif::[]\n\n",
            "ifdef::backend-pdf[]\n== PDFONLY\nendif::[]\n\n",
            "== Details\n\nSee <<_details>>.\n\nimage::assets/chart.svg[Chart]\n"
        );
        let requests = Rc::new(RefCell::new(Vec::new()));
        let mut client = client_with_preview(true, Rc::clone(&requests))?;
        client.open_document(&uri, source)?;
        let offered = actions(&mut client, &uri, &json!(["source"]))?;
        assert!(has_preview(&offered));
        assert!(!has_preview(&actions(
            &mut client,
            &uri,
            &json!(["quickfix"])
        )?));
        assert_eq!(execute(&mut client, &json!([uri]))?, Value::Null);
        let preview = preview_uri(&requests)?;
        let path = preview.to_file_path().ok_or("expected local preview")?;
        #[cfg(unix)]
        assert_eq!(path, path.canonicalize()?);
        let html = fs::read_to_string(&path)?;
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("UNSAVEDBUFFER"));
        assert!(html.contains("HTMLONLY"));
        assert!(!html.contains("PDFONLY"));
        assert!(!html.contains("DISKONLY"));
        assert!(html.contains(&format!("<base href=\"{}\">", uri.replace('&', "&amp;"))));
        assert!(html.contains("src=\"assets/chart.svg\""));
        assert!(html.contains(&format!("href=\"{}#_details\"", preview.as_str())));
        let symbols = client
            .send_request(
                "textDocument/documentSymbol",
                json!({"textDocument": {"uri": uri}}),
            )?
            .to_string();
        assert!(symbols.contains("PDFONLY"));
        assert!(!symbols.contains("HTMLONLY"));

        client.send_notification(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": 2 },
                "contentChanges": [{ "text": "= Revised buffer\n\nREFRESHEDBUFFER\n" }]
            }),
        )?;
        execute(&mut client, &json!([uri]))?;
        assert_eq!(preview_uri(&requests)?, preview);
        let refreshed = fs::read_to_string(&path)?;
        assert!(refreshed.contains("REFRESHEDBUFFER"));
        assert!(!refreshed.contains("UNSAVEDBUFFER"));
        assert!(fs::read_to_string(project.path().join(name))?.contains("DISKONLY"));
        client.send_notification(
            "textDocument/didClose",
            json!({"textDocument": {"uri": uri}}),
        )?;
        assert!(execute(&mut client, &json!([uri])).is_err());
        assert!(path.is_file());
        client.shutdown();
        assert!(!path.exists());
        Ok(())
    }

    #[test]
    fn preview_uses_unsaved_includes_without_loading_stylesheet_or_docinfo_files() -> TestResult {
        let project = Project::new("preview-no-file-reads")?;
        fs::write(project.path().join("part.adoc"), "INCLUDESECRET\n")?;
        fs::write(project.path().join("custom.css"), "CSSSECRET")?;
        fs::write(project.path().join("docinfo.html"), "DOCINFOSECRET")?;
        let uri = project.uri("doc.adoc")?;
        let source = format!(
            "= Preview\n:stylesheet: {}\n:docinfo: shared\n:docinfodir: {}\n:csp!:\n\ninclude::{}[]\n",
            project.path().join("custom.css").display(),
            project.path().display(),
            project.path().join("part.adoc").display(),
        );
        let requests = Rc::new(RefCell::new(Vec::new()));
        let mut client = client_with_preview(true, Rc::clone(&requests))?;
        client.open_document(&project.uri("part.adoc")?, "EDITORINCLUDE\n")?;
        client.open_document(&uri, &source)?;
        execute(&mut client, &json!([uri]))?;
        let html = fs::read_to_string(
            preview_uri(&requests)?
                .to_file_path()
                .ok_or("invalid preview path")?,
        )?;
        for secret in ["INCLUDESECRET", "CSSSECRET", "DOCINFOSECRET"] {
            assert!(!html.contains(secret), "{secret}");
        }
        assert!(html.contains("EDITORINCLUDE"));
        assert!(html.contains("Content-Security-Policy"));
        assert!(html.contains("<style>"));
        client.shutdown();
        Ok(())
    }

    #[test]
    fn preview_rejects_invalid_arguments_and_closed_documents() -> TestResult {
        let requests = Rc::new(RefCell::new(Vec::new()));
        let mut client = client_with_preview(true, Rc::clone(&requests))?;
        for arguments in [
            json!([]),
            json!([12]),
            json!(["invalid uri"]),
            json!(["untitled:one", "untitled:two"]),
            json!(["untitled:closed"]),
        ] {
            let Err(HarnessError::Lsp { error, .. }) = execute(&mut client, &arguments) else {
                return Err("expected invalid params".into());
            };
            assert_eq!(error.get("code"), Some(&json!(-32602)));
        }
        assert!(requests.borrow().is_empty());
        client.shutdown();
        Ok(())
    }

    #[test]
    fn preview_reports_when_the_client_cannot_open_the_browser() -> TestResult {
        let requests = Rc::new(RefCell::new(Vec::new()));
        let mut client = client_with_preview(false, Rc::clone(&requests))?;
        let uri = "untitled:preview.adoc";
        client.open_document(uri, "= Unsaved document\n\nPREVIEWTEXT\n")?;
        let Err(HarnessError::Lsp { error, .. }) = execute(&mut client, &json!([uri])) else {
            return Err("expected preview request failure".into());
        };
        assert_eq!(error.get("code"), Some(&json!(-32803)));
        let html = fs::read_to_string(
            preview_uri(&requests)?
                .to_file_path()
                .ok_or("invalid preview path")?,
        )?;
        assert!(html.contains("PREVIEWTEXT"));
        client.shutdown();
        Ok(())
    }

    #[test]
    fn preview_cannot_bypass_the_document_size_limit() -> TestResult {
        let requests = Rc::new(RefCell::new(Vec::new()));
        let mut client = client_with_preview(true, Rc::clone(&requests))?;
        let uri = "untitled:large.adoc";
        client.open_document(uri, &"x".repeat(10 * 1024 * 1024 + 1))?;
        assert!(!has_preview(&actions(
            &mut client,
            uri,
            &json!(["source"])
        )?));
        assert!(execute(&mut client, &json!([uri])).is_err());
        assert!(requests.borrow().is_empty());
        client.shutdown();
        Ok(())
    }
}
