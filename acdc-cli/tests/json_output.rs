use std::{
    error::Error,
    fs,
    io::{self, Write},
    process::{Command, Output, Stdio},
};

use serde_json::{Value, json};

fn run(args: &[&str], input: &str) -> Result<Output, Box<dyn Error>> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_acdc"))
        .args(args)
        .env_remove("ACDC_LOG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let write_result = child
        .stdin
        .take()
        .ok_or("stdin")?
        .write_all(input.as_bytes());
    let output = child.wait_with_output()?;
    // Early rejection can close stdin before the test finishes writing input.
    if let Err(error) = write_result
        && (error.kind() != io::ErrorKind::BrokenPipe || output.status.success())
    {
        return Err(error.into());
    }
    Ok(output)
}

#[test]
fn native_json_is_available_in_every_build() -> Result<(), Box<dyn Error>> {
    let output = run(&["parse"], "Text.")?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(value.get("name"), Some(&json!("document")));
    assert!(value.get("profile").is_none());
    assert_eq!(
        value.pointer("/blocks/0/inlines/0/value"),
        Some(&json!("Text."))
    );
    Ok(())
}

#[test]
fn file_and_stdin_use_the_selected_language_mode() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("input.adoc");
    fs::write(&path, "[title=\"*Title*\"]\nText.")?;
    for (mode, name, text) in [
        ("asciidoctor", "span", "Title"),
        ("draft", "raw", "*Title*"),
    ] {
        for input in ["-", path.to_str().ok_or("path")?] {
            let output = run(
                &["parse", "--compatibility-mode", mode, input],
                "[title=\"*Title*\"]\nText.",
            )?;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let value: Value = serde_json::from_slice(&output.stdout)?;
            assert_eq!(value.pointer("/blocks/0/title/0/name"), Some(&json!(name)));
            let pointer = if mode == "draft" {
                "/blocks/0/title/0/value"
            } else {
                "/blocks/0/title/0/inlines/0/value"
            };
            assert_eq!(value.pointer(pointer), Some(&json!(text)));
        }
    }
    Ok(())
}

#[cfg(feature = "asg")]
#[test]
fn output_formats_are_independent_of_parser_mode() -> Result<(), Box<dyn Error>> {
    for mode in ["asciidoctor", "draft"] {
        for format in ["native-json", "upstream-asg", "acdc-asg-v1"] {
            let output = run(
                &["parse", "--format", format, "--compatibility-mode", mode],
                "[title=\"\"]\nText.",
            )?;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let value: Value = serde_json::from_slice(&output.stdout)?;
            assert_eq!(value.pointer("/blocks/0/title"), Some(&json!([])));
            assert_eq!(
                value.get("profile"),
                (format == "acdc-asg-v1")
                    .then(|| json!("urn:acdc:asg:1"))
                    .as_ref()
            );
        }
    }
    Ok(())
}

#[cfg(feature = "asg")]
#[test]
fn strict_failure_preserves_output_files_and_reports_structured_context()
-> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("output.json");
    fs::write(&path, "keep")?;
    let source = "Before.\n\nimage:photo.png[Alt]";
    let output = run(
        &[
            "parse",
            "--format",
            "upstream-asg",
            "-o",
            path.to_str().ok_or("path")?,
        ],
        source,
    )?;
    assert!(!output.status.success());
    assert_eq!(output.stdout, [] as [u8; 0]);
    assert_eq!(fs::read_to_string(path)?, "keep");
    let error: Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(error.get("kind"), Some(&json!("unsupported-asg")));
    assert_eq!(error.get("path"), Some(&json!("/blocks/1/inlines/0")));
    assert_eq!(error.get("construct"), Some(&json!("inline image")));
    assert_eq!(error.pointer("/location/0/line"), Some(&json!(3)));
    let output = run(&["parse", "--format", "acdc-asg-v1"], source)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(
        value.pointer("/blocks/1/inlines/0/name"),
        Some(&json!("image"))
    );
    Ok(())
}

#[test]
fn inline_input_and_parser_warnings_keep_stdout_as_json() -> Result<(), Box<dyn Error>> {
    let output = run(&["parse", "--type", "inline"], "*Bold*")?;
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(value.pointer("/0/variant"), Some(&json!("strong")));
    let output = run(&["parse"], "= Title\n:toc2:\n\nText.")?;
    assert!(output.status.success());
    let _: Value = serde_json::from_slice(&output.stdout)?;
    let warning: Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(warning.get("severity"), Some(&json!("warning")));
    assert!(
        warning
            .get("message")
            .and_then(Value::as_str)
            .is_some_and(|s| s.contains("toc2"))
    );
    Ok(())
}

#[cfg(not(feature = "asg"))]
#[test]
fn unavailable_asg_formats_fail_before_writing_output() -> Result<(), Box<dyn Error>> {
    // Exceed pipe capacity to exercise rejection while stdin is still being written.
    let input = "Text.\n".repeat(256 * 1024);
    for format in ["upstream-asg", "acdc-asg-v1"] {
        let output = run(&["parse", "--format", format], &input)?;
        assert!(!output.status.success());
        assert_eq!(output.stdout, [] as [u8; 0]);
        let error: Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(error.get("kind"), Some(&json!("unavailable-format")));
        assert!(
            error
                .get("message")
                .and_then(Value::as_str)
                .is_some_and(|s| s.contains("asg"))
        );
    }
    Ok(())
}

#[cfg(feature = "asg")]
#[test]
fn native_and_asg_presence_values_keep_their_distinct_contracts() -> Result<(), Box<dyn Error>> {
    for format in ["native-json", "upstream-asg", "acdc-asg-v1"] {
        let output = run(
            &["parse", "--format", format],
            "= Title\n:flag:\n:unset!:\n\nText.",
        )?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout)?;
        let (set, unset) = if format == "native-json" {
            (json!(true), None)
        } else {
            (json!(""), Some(Value::Null))
        };
        assert_eq!(value.pointer("/attributes/flag"), Some(&set));
        assert_eq!(value.pointer("/attributes/unset"), unset.as_ref());
    }
    Ok(())
}

#[test]
fn file_includes_use_the_entry_directory_and_include_budget() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("entry.adoc");
    fs::write(&path, "include::part.adoc[]")?;
    fs::write(directory.path().join("part.adoc"), "Text.\n")?;
    let output = run(&["parse", path.to_str().ok_or("path")?], "")?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(
        value.pointer("/blocks/0/inlines/0/value"),
        Some(&json!("Text."))
    );
    let output = run(
        &[
            "parse",
            "--max-total-include-bytes",
            "1",
            path.to_str().ok_or("path")?,
        ],
        "",
    )?;
    assert!(!output.status.success());
    assert_eq!(output.stdout, [] as [u8; 0]);
    let error: Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(error.get("kind"), Some(&json!("parse")));
    Ok(())
}

#[cfg(feature = "tck")]
#[test]
fn tck_uses_draft_and_does_not_accept_other_output_formats() -> Result<(), Box<dyn Error>> {
    for (kind, source) in [
        ("block", "[title=\"*Title*\"]\nText."),
        ("inline", "*Bold*"),
    ] {
        let input = json!({"contents":source,"path":"test.adoc","type":kind}).to_string();
        let output = run(&["tck"], &input)?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout)?;
        assert!(value.get("profile").is_none());
        if kind == "block" {
            assert_eq!(
                value.pointer("/blocks/0/title/0/name"),
                Some(&json!("text"))
            );
        } else {
            assert_eq!(value.pointer("/0/variant"), Some(&json!("strong")));
        }
    }
    for args in [
        vec!["tck", "--format", "native-json"],
        vec!["tck", "--compatibility-mode", "asciidoctor"],
    ] {
        let output = run(&args, "")?;
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(output.stdout, [] as [u8; 0]);
    }
    let output = run(&["tck"], "not JSON")?;
    assert!(!output.status.success());
    assert_eq!(output.stdout, [] as [u8; 0]);
    let error: Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(error.get("kind"), Some(&json!("invalid-json")));
    Ok(())
}

#[cfg(feature = "tck")]
fn tck(source: &str) -> Result<Value, Box<dyn Error>> {
    let input = json!({"contents":source,"path":"test.adoc","type":"block"}).to_string();
    let output = run(&["tck"], &input)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[cfg(feature = "tck")]
#[test]
fn tck_omits_only_unreferenced_generated_heading_ids() -> Result<(), Box<dyn Error>> {
    for (source, expected) in [
        ("== Generated\n\nText.", None),
        ("[#explicit]\n== Explicit\n\nText.", Some("explicit")),
        ("[#_explicit]\n== Explicit\n\nText.", Some("_explicit")),
        ("== Generated\n\nSee <<_generated>>.", Some("_generated")),
        (
            "== Generated\n\nSee link:./#_generated[here].",
            Some("_generated"),
        ),
    ] {
        let graph = tck(source)?;
        assert_eq!(
            graph.pointer("/blocks/0/id").and_then(Value::as_str),
            expected,
            "{source}"
        );
        let ordinary = run(
            &[
                "parse",
                "--format",
                "upstream-asg",
                "--compatibility-mode",
                "draft",
            ],
            source,
        )?;
        assert!(
            ordinary.status.success(),
            "{}",
            String::from_utf8_lossy(&ordinary.stderr)
        );
        let ordinary: Value = serde_json::from_slice(&ordinary.stdout)?;
        assert!(ordinary.pointer("/blocks/0/id").is_some());
    }
    let graph = tck("== Parent\n\n=== Child\n\nText.")?;
    assert!(graph.pointer("/blocks/0/id").is_none());
    assert!(graph.pointer("/blocks/0/blocks/0/id").is_none());
    Ok(())
}

#[cfg(feature = "tck")]
#[test]
fn tck_omits_derived_captions_without_changing_regular_exports() -> Result<(), Box<dyn Error>> {
    for source in [
        "----\ndef main\n  puts 'hello'\nend\n----",
        ".Title\n----\nCode\n----",
        "[#code]\n.Title\n----\nCode\n----\n\nSee <<code>>.",
        "== Section\n\n----\nCode\n----",
        "* Item\n+\n----\nCode\n----",
    ] {
        let graph = tck(source)?;
        assert!(graph.get("profile").is_none());
        let ordinary = run(
            &[
                "parse",
                "--format",
                "upstream-asg",
                "--compatibility-mode",
                "draft",
            ],
            source,
        )?;
        assert!(!ordinary.status.success(), "{source}");
        let diagnostic: Value = serde_json::from_slice(&ordinary.stderr)?;
        assert_eq!(diagnostic.get("kind"), Some(&json!("unsupported-asg")));
        assert!(
            diagnostic
                .get("path")
                .and_then(Value::as_str)
                .is_some_and(|path| path.ends_with("/caption"))
        );
        let rich = run(
            &[
                "parse",
                "--format",
                "acdc-asg-v1",
                "--compatibility-mode",
                "draft",
            ],
            source,
        )?;
        assert!(rich.status.success());
        let rich: Value = serde_json::from_slice(&rich.stdout)?;
        assert!(rich.to_string().contains("\"caption\""));
    }
    let graph = tck("[#code]\n.Title\n----\nCode\n----")?;
    assert_eq!(graph.pointer("/blocks/0/id"), Some(&json!("code")));
    assert_eq!(
        graph.pointer("/blocks/0/title/0/value"),
        Some(&json!("Title"))
    );
    assert!(graph.pointer("/blocks/0/caption").is_none());
    Ok(())
}

#[cfg(feature = "tck")]
#[test]
fn tck_retains_authored_caption_settings_and_rejects_other_unsupported_content()
-> Result<(), Box<dyn Error>> {
    let graph = tck("= Doc\n:listing-caption: Program\n\n.Title\n----\nCode\n----")?;
    assert_eq!(
        graph.pointer("/attributes/listing-caption"),
        Some(&json!("Program"))
    );
    assert_eq!(
        graph.pointer("/blocks/0/title/0/value"),
        Some(&json!("Title"))
    );
    for source in [
        "[caption='*Custom* ']\n.Title\n----\nCode\n----",
        "image:bird.svg[Bird]",
        "[quote,Author,Book]\nQuotation.",
    ] {
        let input = json!({"contents":source,"path":"test.adoc","type":"block"}).to_string();
        let output = run(&["tck"], &input)?;
        assert!(!output.status.success(), "{source}");
        assert_eq!(output.stdout, [] as [u8; 0]);
        let diagnostic: Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(diagnostic.get("kind"), Some(&json!("unsupported-asg")));
    }
    Ok(())
}
