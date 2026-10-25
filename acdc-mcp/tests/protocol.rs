use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::{process::Command, time::timeout};

fn server_command(current_directory: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_acdc-mcp"));
    command
        .current_dir(current_directory)
        .env_remove("CLAUDE_PROJECT_DIR");
    command
}

fn root_document(root: &Path, name: &str) -> PathBuf {
    let path = root.join(name);
    fs::write(&path, "= Guide\n\n== Root selection\n\nContent.\n").unwrap();
    path.canonicalize().unwrap()
}

async fn assert_roots(command: Command, readable: &[PathBuf], blocked: &[PathBuf]) {
    timeout(Duration::from_secs(20), async {
        let client = ().serve(TokioChildProcess::new(command).unwrap()).await.unwrap();
        for (allowed, paths) in [(true, readable), (false, blocked)] {
            for path in paths {
                let input = if allowed {
                    Path::new(path.file_name().unwrap())
                } else {
                    path.as_path()
                };
                let result = client
                    .call_tool(
                        CallToolRequestParams::new("adoc_tree")
                            .with_arguments(json!({"path": input}).as_object().unwrap().clone()),
                    )
                    .await
                    .unwrap();
                let result = serde_json::to_value(result).unwrap();
                assert_eq!(
                    result["isError"] == true,
                    !allowed,
                    "{}: {result}",
                    path.display()
                );
                if allowed {
                    let page: Value =
                        serde_json::from_str(result["content"][0]["text"].as_str().unwrap())
                            .unwrap();
                    assert_eq!(page["items"][0]["path"], json!(path));
                } else {
                    assert!(
                        result["content"][0]["text"]
                            .as_str()
                            .unwrap()
                            .contains("outside the configured roots"),
                        "{result}"
                    );
                }
            }
        }
        let result = client
            .call_tool(
                CallToolRequestParams::new("adoc_find").with_arguments(
                    json!({"query": "Root selection"})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        let result = serde_json::to_value(result).unwrap();
        assert_ne!(result["isError"], true, "{result}");
        let page: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        let items = page["items"].as_array().unwrap();
        let paths = items
            .iter()
            .filter(|item| item["kind"] == "match")
            .map(|item| PathBuf::from(item["path"].as_str().unwrap()))
            .collect::<BTreeSet<_>>();
        assert_eq!(paths, readable.iter().cloned().collect());
        assert_eq!(items[0]["documents_scanned"], json!(readable.len()));
        assert_eq!(items[0]["matches"], json!(readable.len()));
        assert_eq!(items.len(), readable.len() + 1);
        assert_eq!(page["next_cursor"], Value::Null);
        client.cancel().await.unwrap();
    })
    .await
    .expect("root selection protocol test timed out");
}

#[tokio::test]
async fn stdio_defaults_to_the_startup_directory() {
    let current = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let current_document = root_document(current.path(), "current.adoc");
    let outside_document = root_document(outside.path(), "outside.adoc");
    assert_roots(
        server_command(current.path()),
        &[current_document],
        &[outside_document],
    )
    .await;
}

#[tokio::test]
async fn stdio_explicit_roots_add_to_the_startup_directory_without_duplicates() {
    let current = TempDir::new().unwrap();
    let first = TempDir::new().unwrap();
    let second = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let documents = [
        root_document(current.path(), "current.adoc"),
        root_document(first.path(), "first.adoc"),
        root_document(second.path(), "second.adoc"),
    ];
    let outside_document = root_document(outside.path(), "outside.adoc");
    let mut command = server_command(current.path());
    command
        .arg("--root")
        .arg(first.path())
        .arg("--root")
        .arg(second.path())
        .args(["--root", ".", "--root", "."]);
    assert_roots(command, &documents, &[outside_document]).await;
}

#[tokio::test]
async fn stdio_uses_the_claude_project_directory_with_or_without_extra_roots() {
    let current = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    let extra = TempDir::new().unwrap();
    let current_document = root_document(current.path(), "current.adoc");
    let project_document = root_document(project.path(), "project.adoc");
    let extra_document = root_document(extra.path(), "extra.adoc");
    for add_extra in [false, true] {
        let mut command = server_command(current.path());
        command.env("CLAUDE_PROJECT_DIR", project.path());
        let mut readable = vec![project_document.clone()];
        let mut blocked = vec![current_document.clone()];
        if add_extra {
            command.arg("--root").arg(extra.path());
            readable.push(extra_document.clone());
        } else {
            blocked.push(extra_document.clone());
        }
        assert_roots(command, &readable, &blocked).await;
    }
}

#[tokio::test]
async fn stdio_strict_roots_exclude_startup_and_claude_project_directories() {
    let current = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    let first = TempDir::new().unwrap();
    let second = TempDir::new().unwrap();
    let readable = [
        root_document(first.path(), "first.adoc"),
        root_document(second.path(), "second.adoc"),
    ];
    let blocked = [
        root_document(current.path(), "current.adoc"),
        root_document(project.path(), "project.adoc"),
    ];
    let missing_project = project.path().join("missing");
    for project_directory in [None, Some(project.path()), Some(missing_project.as_path())] {
        let mut command = server_command(current.path());
        command
            .arg("--strictly-root-folders")
            .arg("--root")
            .arg(first.path())
            .arg("--root")
            .arg(second.path());
        if let Some(directory) = project_directory {
            command.env("CLAUDE_PROJECT_DIR", directory);
        }
        assert_roots(command, &readable, &blocked).await;
    }
}

#[tokio::test]
async fn stdio_client_discovers_and_calls_all_tools() {
    timeout(Duration::from_secs(20), async {
        let root = TempDir::new().unwrap();
        fs::write(root.path().join("guide.adoc"), "= Guide\nAda Lovelace\nv1.0, 2026-10-20\n\n[#install]\n== Installation\n\nInstall *{product}*. See <<install>> and https://example.com[Website].\n\n.Run command\n[#command]\n[source,sh]\n----\necho hello\n----\n").unwrap();
        let mut command = server_command(root.path());
        command.arg("--root").arg(root.path()).args(["-a", "product=Widget"]);
        let transport = TokioChildProcess::new(command).unwrap();
        let client = ().serve(transport).await.unwrap();
        let peer_info = client.peer_info().unwrap();
        let info = peer_info.server_info.as_ref().unwrap();
        assert_eq!(info.name, "acdc-mcp");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        let tools = client.list_all_tools().await.unwrap();
        let mut names = tools.iter().map(|tool| tool.name.as_ref()).collect::<Vec<_>>();
        names.sort_unstable();
        assert_eq!(names, ["adoc_block", "adoc_find", "adoc_links", "adoc_metadata", "adoc_resolve", "adoc_section", "adoc_tree"]);
        for tool in &tools {
            assert_eq!(tool.annotations.as_ref().unwrap().read_only_hint, Some(true));
        }
        let calls = [
            ("adoc_tree", json!({"path":"guide.adoc"}), "Installation"),
            ("adoc_find", json!({"query":"instl"}), "Installation"),
            ("adoc_section", json!({"path":"guide.adoc", "selector":"install", "format":"plain", "attributes":{"product":"Gadget", "enabled":true, "disabled":false}}), "Install Gadget"),
            ("adoc_section", json!({"path":"guide.adoc", "selector":"instl", "format":"plain"}), "Install Widget"),
            ("adoc_block", json!({"path":"guide.adoc", "selector":"command"}), "echo hello"),
            ("adoc_metadata", json!({"path":"guide.adoc"}), "Lovelace"),
            ("adoc_links", json!({"path":"guide.adoc"}), "https://example.com"),
            ("adoc_resolve", json!({"path":"guide.adoc", "target":"#install"}), "resolved"),
        ];
        for (name, arguments, expected) in calls {
            let result = client.call_tool(CallToolRequestParams::new(name).with_arguments(arguments.as_object().unwrap().clone())).await.unwrap();
            let result = serde_json::to_value(result).unwrap();
            assert_ne!(result["isError"], true, "{result}");
            let text = result["content"][0]["text"].as_str().unwrap();
            let page: Value = serde_json::from_str(text).unwrap();
            assert!(page["items"].is_array());
            assert!(text.contains(expected), "{name}: {text}");
        }
        let result = client.call_tool(CallToolRequestParams::new("adoc_tree").with_arguments(json!({"path":"../outside.adoc"}).as_object().unwrap().clone())).await.unwrap();
        assert_eq!(serde_json::to_value(result).unwrap()["isError"], true);
        client.cancel().await.unwrap();
    }).await.expect("stdio protocol test timed out");
}

#[tokio::test]
async fn stdio_negotiates_older_client_protocol_versions() {
    timeout(Duration::from_secs(20), async {
        for version in [
            rmcp::model::ProtocolVersion::V_2024_11_05,
            rmcp::model::ProtocolVersion::V_2025_11_25,
        ] {
            let root = TempDir::new().unwrap();
            let mut command = server_command(root.path());
            command.arg("--root").arg(root.path());
            let client = rmcp::model::ClientConfig::default()
                .with_protocol_version(version.clone())
                .serve(TokioChildProcess::new(command).unwrap())
                .await
                .unwrap();
            assert_eq!(client.peer_info().unwrap().protocol_version, version);
            assert_eq!(client.list_all_tools().await.unwrap().len(), 7);
            client.cancel().await.unwrap();
        }
    })
    .await
    .expect("legacy protocol test timed out");
}
