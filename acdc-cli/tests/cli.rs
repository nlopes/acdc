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
fn command_line_attributes_cannot_be_changed_by_document_entries()
-> Result<(), Box<dyn std::error::Error>> {
    let locked_set = run_acdc(
        &[
            "convert",
            "--embedded",
            "--stdin",
            "--out-file",
            "-",
            "-a",
            "experimental",
        ],
        Some("Before.\n\n:experimental!:\n\nkbd:[Ctrl+C]\n"),
    )?;
    let locked_unset = run_acdc(
        &[
            "convert",
            "--embedded",
            "--stdin",
            "--out-file",
            "-",
            "-a",
            "experimental!",
        ],
        Some("Before.\n\n:experimental:\n\nkbd:[Ctrl+C]\n"),
    )?;
    let set_output = output_text(&locked_set.stdout);
    let unset_output = output_text(&locked_unset.stdout);

    assert!(
        locked_set.status.success(),
        "{}",
        output_text(&locked_set.stderr)
    );
    assert!(
        locked_unset.status.success(),
        "{}",
        output_text(&locked_unset.stderr)
    );
    assert!(set_output.contains("<kbd>Ctrl</kbd>+<kbd>C</kbd>"));
    assert!(unset_output.contains("kbd:[Ctrl+C]"));
    assert!(!unset_output.contains("<kbd>"));
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn soft_command_line_attributes_can_be_changed_by_document_entries()
-> Result<(), Box<dyn std::error::Error>> {
    let output = run_acdc(
        &[
            "convert",
            "--embedded",
            "--stdin",
            "--out-file",
            "-",
            "-a",
            "project@=api",
            "-a",
            "team=api@",
            "-a",
            "!feature=@",
            "-a",
            "!mode@",
            "-a",
            "suffix!@",
            "-a",
            "removed@=api",
        ],
        Some(
            ":project: document\n\
             :team: document\n\
             :feature: document\n\
             :mode: document\n\
             :suffix: document\n\
             :removed!:\n\n\
             {project}|{team}|{feature}|{mode}|{suffix}|{removed}\n",
        ),
    )?;
    let converted = output_text(&output.stdout);

    assert!(output.status.success(), "{}", output_text(&output.stderr));
    assert!(converted.contains("document|document|document|document|document|{removed}"));
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn converter_defaults_remain_document_overridable() -> Result<(), Box<dyn std::error::Error>> {
    let output = run_acdc(
        &["convert", "--stdin", "--out-file", "-"],
        Some("= T\n:lang: fr\n\nBody.\n"),
    )?;
    let converted = output_text(&output.stdout);

    assert!(output.status.success(), "{}", output_text(&output.stderr));
    assert!(converted.contains("<html lang=\"fr\">"));
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn implied_and_conversion_only_attributes_are_not_seeded_in_the_parser()
-> Result<(), Box<dyn std::error::Error>> {
    let output = run_acdc(
        &["convert", "--stdin", "--out-file", "-"],
        Some("= T\n\n{lang}|{outdir}|{outfile}\n"),
    )?;
    let converted = output_text(&output.stdout);

    assert!(output.status.success(), "{}", output_text(&output.stderr));
    assert!(converted.contains("<html lang=\"en\">"));
    assert!(converted.contains("{lang}|{outdir}|{outfile}"));
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

#[cfg(all(
    feature = "html",
    feature = "manpage",
    feature = "markdown",
    feature = "pdf",
    feature = "terminal",
))]
#[test]
fn all_backends_render_recovered_bibliography_children() -> Result<(), Box<dyn std::error::Error>> {
    const SOURCE: &str = "= recovery(1)\n\n\
        == NAME\n\n\
        recovery - test bibliography recovery\n\n\
        == SYNOPSIS\n\n\
        recovery\n\n\
        [bibliography]\n\
        == REFERENCES\n\n\
        === Recovered Child\n\n\
        Recovered child body.\n\n\
        == FOLLOWING\n\n\
        Following body.\n";
    const WARNING: &str = "bibliography sections do not support nested sections";

    let temp = tempfile::tempdir()?;
    let document = temp.path().join("recovery.adoc");
    fs::write(&document, SOURCE)?;
    let document_arg = document.to_string_lossy();

    let assert_rendered = |rendered: &str| {
        assert!(rendered.contains("Recovered Child"), "{rendered}");
        assert!(rendered.contains("Recovered child body."), "{rendered}");
    };
    let assert_warning = |output: &Output| {
        let stderr = output_text(&output.stderr);
        assert!(stderr.contains(WARNING), "{stderr}");
    };

    for backend in ["html", "markdown", "manpage", "terminal"] {
        let output = run_acdc(
            &[
                "convert",
                "--backend",
                backend,
                "--out-file",
                "-",
                document_arg.as_ref(),
            ],
            None,
        )?;

        assert!(output.status.success(), "{}", output_text(&output.stderr));
        assert_warning(&output);
        assert_rendered(&output_text(&output.stdout));
    }

    let pdf_path = temp.path().join("recovery.pdf");
    let pdf_arg = pdf_path.to_string_lossy();
    let typst_path = temp.path().join("recovery.typ");
    let typst_arg = typst_path.to_string_lossy();
    let pdf = run_acdc(
        &[
            "convert",
            "--backend",
            "pdf",
            "--emit-typst",
            typst_arg.as_ref(),
            "--out-file",
            pdf_arg.as_ref(),
            document_arg.as_ref(),
        ],
        None,
    )?;
    assert!(pdf.status.success(), "{}", output_text(&pdf.stderr));
    assert_warning(&pdf);
    assert_rendered(&fs::read_to_string(typst_path)?);
    assert!(fs::read(pdf_path)?.starts_with(b"%PDF-"));

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
