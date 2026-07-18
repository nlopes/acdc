use std::{
    io::{self, Write},
    process::{Command, Output, Stdio},
};

#[cfg(any(feature = "html", feature = "terminal", feature = "inspect"))]
use std::fs;

fn run_acdc(args: &[&str], input: Option<&str>) -> io::Result<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_acdc"));
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    }

    let mut child = command.spawn()?;
    if let Some(input) = input {
        let Some(mut stdin) = child.stdin.take() else {
            return Err(io::Error::other("acdc stdin was not piped"));
        };
        stdin.write_all(input.as_bytes())?;
    }
    child.wait_with_output()
}

fn output_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(not(any(
    feature = "html",
    feature = "manpage",
    feature = "markdown",
    feature = "pdf",
    feature = "terminal",
    feature = "inspect",
    feature = "lint",
    feature = "tck",
)))]
#[test]
fn no_command_features_return_a_clear_diagnostic() -> Result<(), Box<dyn std::error::Error>> {
    let output = run_acdc(&[], None)?;
    let stderr = output_text(&output.stderr);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr.contains("built without any subcommand features"));
    assert!(stderr.contains("pdf"));
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn convert_requires_an_input() -> Result<(), Box<dyn std::error::Error>> {
    let output = run_acdc(&["convert"], None)?;
    let stderr = output_text(&output.stderr);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr.contains("required arguments were not provided"));
    assert!(stderr.contains("Usage: acdc convert"));
    Ok(())
}

#[cfg(feature = "lint")]
#[test]
fn lint_requires_an_input() -> Result<(), Box<dyn std::error::Error>> {
    let output = run_acdc(&["lint"], None)?;
    let stderr = output_text(&output.stderr);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr.contains("required arguments were not provided"));
    assert!(stderr.contains("Usage: acdc lint"));
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn missing_input_file_returns_a_failure() -> Result<(), Box<dyn std::error::Error>> {
    let missing = "acdc-cli-test-file-that-does-not-exist.adoc";
    let output = run_acdc(&["convert", missing], None)?;
    let stderr = output_text(&output.stderr);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains(missing));
    assert!(stderr.contains("No such file or directory"));
    Ok(())
}

#[cfg(feature = "lint")]
#[test]
fn denied_lint_returns_a_failure() -> Result<(), Box<dyn std::error::Error>> {
    let output = run_acdc(
        &[
            "lint",
            "--stdin",
            "--output-style",
            "compact",
            "--deny",
            "hard-tab",
        ],
        Some("a\thard tab\n"),
    )?;
    let stderr = output_text(&output.stderr);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("deny[hard-tab]"));
    Ok(())
}

#[cfg(feature = "tck")]
#[test]
fn invalid_tck_type_returns_a_failure() -> Result<(), Box<dyn std::error::Error>> {
    let output = run_acdc(
        &["tck"],
        Some(r#"{"contents":"text","path":"test.adoc","type":"document"}"#),
    )?;
    let stderr = output_text(&output.stderr);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("unsupported TCK type `document`"));
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn converts_stdin_to_stdout() -> Result<(), Box<dyn std::error::Error>> {
    let output = run_acdc(
        &["convert", "--stdin", "--out-file", "-"],
        Some("= CLI integration test\n\nConverted body.\n"),
    )?;
    let stdout = output_text(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("<!DOCTYPE html>"));
    assert!(stdout.contains("Converted body."));
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn selected_backend_attributes_are_available_during_parsing()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let document = temp.path().join("backend-attributes.adoc");
    fs::write(
        &document,
        "ifdef::backend-html5-doctype-book[]\n\
         backend={backend}; basebackend={basebackend}; filetype={filetype}; \
         outfilesuffix={outfilesuffix}; htmlsyntax={htmlsyntax}\n\
         endif::[]\n\
         ifdef::backend-pdf[]\n\
         wrong backend\n\
         endif::[]\n",
    )?;
    let document_arg = document.to_string_lossy();

    let output = run_acdc(
        &["convert", "--doctype", "book", document_arg.as_ref()],
        None,
    )?;
    let converted = fs::read_to_string(document.with_extension("html"))?;

    assert!(output.status.success(), "{}", output_text(&output.stderr));
    assert!(converted.contains(
        "backend=html5; basebackend=html; filetype=html; outfilesuffix=.html; htmlsyntax=html"
    ));
    assert!(!converted.contains("wrong backend"));
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn converts_multiple_files_with_a_timing_summary() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let first = temp.path().join("first.adoc");
    let second = temp.path().join("second.adoc");
    fs::write(&first, "= First\n\nFirst body.\n")?;
    fs::write(&second, "= Second\n\nSecond body.\n")?;
    let first_arg = first.to_string_lossy();
    let second_arg = second.to_string_lossy();

    let output = run_acdc(
        &[
            "convert",
            "--timings",
            first_arg.as_ref(),
            second_arg.as_ref(),
        ],
        None,
    )?;
    let stderr = output_text(&output.stderr);

    assert!(output.status.success());
    assert!(first.with_extension("html").is_file());
    assert!(second.with_extension("html").is_file());
    assert!(stderr.contains("Total (2 files)"));
    assert!(stderr.contains("Wall clock"));
    Ok(())
}

#[cfg(feature = "terminal")]
#[test]
fn terminal_converts_multiple_files_without_a_pager() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let first = temp.path().join("first.adoc");
    let second = temp.path().join("second.adoc");
    fs::write(&first, "First terminal document.\n")?;
    fs::write(&second, "Second terminal document.\n")?;
    let first_arg = first.to_string_lossy();
    let second_arg = second.to_string_lossy();

    let output = run_acdc(
        &[
            "convert",
            "--backend",
            "terminal",
            "--no-pager",
            first_arg.as_ref(),
            second_arg.as_ref(),
        ],
        None,
    )?;
    let stdout = output_text(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("First terminal document."));
    assert!(stdout.contains("Second terminal document."));
    Ok(())
}

#[cfg(feature = "inspect")]
#[test]
fn inspect_resolves_includes_and_omits_ansi_when_piped() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let included = temp.path().join("included.adoc");
    let document = temp.path().join("document.adoc");
    fs::write(&included, "Included paragraph.\n")?;
    fs::write(&document, "= Document\n\ninclude::included.adoc[]\n")?;
    let document_arg = document.to_string_lossy();

    let output = run_acdc(&["inspect", document_arg.as_ref()], None)?;
    let stdout = output_text(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("Included paragraph."));
    assert!(!stdout.contains('\u{1b}'));
    Ok(())
}
