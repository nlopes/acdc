#![cfg(feature = "html")]

use std::{
    error::Error,
    fs,
    io::{self, Write},
    path::Path,
    process::{Command, Output, Stdio},
};

type TestResult = Result<(), Box<dyn Error>>;

const SOURCE: &str = "= Lists\n:word: Early\n\nlist-of::quote[]\n\n.A *bold* {word} & title\n[#authored]\n[quote]\n____\nQuoted.\n____\n\n:word: Late\n";

fn run(directory: &Path, args: &[&str], input: Option<&str>) -> Result<Output, Box<dyn Error>> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_acdc"))
        .current_dir(directory)
        .args(args)
        .env_remove("ACDC_LOG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let written = child
        .stdin
        .take()
        .ok_or("stdin")?
        .write_all(input.unwrap_or_default().as_bytes());
    let output = child.wait_with_output()?;
    if let Err(error) = written
        && (error.kind() != io::ErrorKind::BrokenPipe || output.status.success())
    {
        return Err(error.into());
    }
    Ok(output)
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn lists_are_inactive_without_explicit_selection() -> TestResult {
    let directory = tempfile::tempdir()?;
    fs::write(directory.path().join("inactive.adoc"), SOURCE)?;
    for mode in ["asciidoctor", "draft"] {
        for input in ["--stdin", "inactive.adoc"] {
            let output = run(
                directory.path(),
                &[
                    "convert",
                    "--backend",
                    "html",
                    "--compatibility-mode",
                    mode,
                    input,
                    "--embedded",
                    "--fail-on-warnings",
                    "-o",
                    "-",
                ],
                Some(SOURCE),
            )?;
            success(&output);
            let html = String::from_utf8(output.stdout)?;
            assert!(html.contains("list-of::quote[]"));
            assert!(!html.contains("href=\"#authored\""));
        }
    }
    Ok(())
}

#[cfg(not(feature = "lists"))]
#[test]
fn lists_unavailable_selection_fails_before_reading_input() -> TestResult {
    let directory = tempfile::tempdir()?;
    for input in ["missing.adoc", "--stdin"] {
        let output = run(
            directory.path(),
            &[
                "convert",
                "--backend",
                "html",
                "--extension",
                "lists",
                input,
            ],
            Some("list-of::quote[enhanced_rendering=true]"),
        )?;
        assert_eq!(output.status.code(), Some(1));
        let stderr = String::from_utf8(output.stderr)?;
        assert!(
            stderr.contains("extension `lists` is not available"),
            "{stderr}"
        );
        assert!(!stderr.contains("title formatting is automatic"));
        assert_eq!(output.stdout, [] as [u8; 0]);
    }
    Ok(())
}

#[cfg(feature = "lists")]
#[test]
fn lists_files_batches_and_stdin_preserve_formatting_in_both_modes_and_variants() -> TestResult {
    let directory = tempfile::tempdir()?;
    fs::write(directory.path().join("first.adoc"), SOURCE)?;
    fs::write(
        directory.path().join("second.adoc"),
        SOURCE.replace("Early", "Second"),
    )?;
    for mode in ["asciidoctor", "draft"] {
        for variant in ["standard", "semantic"] {
            let mut args = vec![
                "convert",
                "--backend",
                "html",
                "--variant",
                variant,
                "--compatibility-mode",
                mode,
                "--extension",
                "lists",
                "--extension",
                "lists",
                "--embedded",
                "--fail-on-warnings",
            ];
            args.extend(["first.adoc", "second.adoc"]);
            success(&run(directory.path(), &args, None)?);
            for (file, word) in [("first.html", "Early"), ("second.html", "Second")] {
                let html = fs::read_to_string(directory.path().join(file))?;
                assert_eq!(html.matches("href=\"#authored\"").count(), 1, "{html}");
                assert!(html.contains(&format!("A <strong>bold</strong> {word} &amp; title")));
                assert!(!html.contains("list-of::"));
                assert!(!html.contains("&lt;strong&gt;"));
            }
            args.truncate(args.len() - 2);
            args.extend(["--stdin", "-o", "-"]);
            let output = run(directory.path(), &args, Some(SOURCE))?;
            success(&output);
            let html = String::from_utf8(output.stdout)?;
            assert_eq!(html.matches("href=\"#authored\"").count(), 1);
            assert!(html.contains("A <strong>bold</strong> Early &amp; title"));
        }
    }
    Ok(())
}

#[cfg(feature = "lists")]
#[test]
fn lists_warnings_retain_included_locations_and_batch_output() -> TestResult {
    let directory = tempfile::tempdir()?;
    fs::write(directory.path().join("entry.adoc"), "include::part.adoc[]")?;
    fs::write(directory.path().join("part.adoc"), "\n\nlist-of::unknown[]")?;
    fs::write(directory.path().join("good.adoc"), SOURCE)?;
    let output = run(
        directory.path(),
        &[
            "convert",
            "--backend",
            "html",
            "--extension",
            "lists",
            "--embedded",
            "--fail-on-warnings",
            "entry.adoc",
            "good.adoc",
        ],
        None,
    )?;
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(
        stderr.matches("unknown list-of element: unknown").count(),
        1,
        "{stderr}"
    );
    assert!(stderr.contains("part.adoc"), "{stderr}");
    assert!(
        fs::read_to_string(directory.path().join("entry.html"))?.contains("list-of::unknown[]")
    );
    assert!(fs::read_to_string(directory.path().join("good.html"))?.contains("href=\"#authored\""));
    Ok(())
}

#[cfg(feature = "lists")]
#[test]
fn lists_reject_enhanced_rendering_with_migration_guidance() -> TestResult {
    let directory = tempfile::tempdir()?;
    for mode in ["asciidoctor", "draft"] {
        let output = run(
            directory.path(),
            &[
                "convert",
                "--backend",
                "html",
                "--extension",
                "lists",
                "--compatibility-mode",
                mode,
                "--stdin",
                "-o",
                "-",
            ],
            Some("list-of::quote[enhanced_rendering=false]"),
        )?;
        assert_eq!(output.status.code(), Some(1));
        let stderr = String::from_utf8(output.stderr)?;
        assert!(
            stderr.contains("does not support enhanced_rendering"),
            "{stderr}"
        );
        assert!(stderr.contains("remove this option"), "{stderr}");
        assert_eq!(output.stdout, [] as [u8; 0]);
    }
    Ok(())
}

#[cfg(feature = "lists")]
#[test]
fn lists_preserve_escaped_and_verbatim_calls() -> TestResult {
    let directory = tempfile::tempdir()?;
    let source = format!(
        "{SOURCE}\n\\list-of::quote[]\n\n----\nlist-of::quote[]\n----\n\n....\nlist-of::quote[]\n....\n"
    );
    let output = run(
        directory.path(),
        &[
            "convert",
            "--extension",
            "lists",
            "--stdin",
            "--embedded",
            "-o",
            "-",
        ],
        Some(&source),
    )?;
    success(&output);
    let html = String::from_utf8(output.stdout)?;
    assert_eq!(html.matches("href=\"#authored\"").count(), 1, "{html}");
    assert_eq!(html.matches("list-of::quote[]").count(), 3, "{html}");
    Ok(())
}

#[cfg(feature = "lists")]
#[test]
fn lists_render_ordinary_nodes_through_each_enabled_backend() -> TestResult {
    let directory = tempfile::tempdir()?;
    let source = "= lists(1)\n\n== NAME\nlists - native lists\n\n== SYNOPSIS\n*lists*\n\n== DESCRIPTION\n\nlist-of::quote[]\n\n.Uniquetarget\n[#authored]\n[quote]\n____\nQuoted.\n____\n";
    let backends: &[&str] = &[
        #[cfg(feature = "manpage")]
        "manpage",
        #[cfg(feature = "markdown")]
        "markdown",
        #[cfg(feature = "terminal")]
        "terminal",
        #[cfg(feature = "pdf")]
        "pdf",
    ];
    for backend in backends {
        for mode in ["asciidoctor", "draft"] {
            let mut args = vec![
                "convert",
                "--backend",
                backend,
                "--extension",
                "lists",
                "--compatibility-mode",
                mode,
                "--stdin",
                "--fail-on-warnings",
                "-o",
                "-",
            ];
            if *backend == "terminal" {
                args.push("--no-pager");
            }
            let output = run(directory.path(), &args, Some(source))?;
            success(&output);
            if *backend == "pdf" {
                assert!(output.stdout.starts_with(b"%PDF-"));
            } else {
                let text = String::from_utf8(output.stdout)?;
                assert_eq!(text.matches("Uniquetarget").count(), 2, "{backend}: {text}");
                assert!(!text.contains("list-of::"), "{backend}: {text}");
            }
        }
    }
    Ok(())
}
