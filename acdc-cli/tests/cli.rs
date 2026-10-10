use std::{
    error::Error,
    io::{self, Write},
    process::{Command, Output, Stdio},
};
#[cfg(any(feature = "html", feature = "terminal", feature = "inspect"))]
use tempfile::tempdir;

#[cfg(any(
    feature = "html",
    feature = "terminal",
    feature = "inspect",
    feature = "execute",
    feature = "lint"
))]
use std::fs;

fn spawn_acdc(
    working_dir: Option<&std::path::Path>,
    args: &[&str],
    input: Option<&str>,
) -> io::Result<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_acdc"));
    command
        .args(args)
        .env_remove("ACDC_LOG")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(working_dir) = working_dir {
        command.current_dir(working_dir);
    }
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

fn run_acdc(args: &[&str], input: Option<&str>) -> io::Result<Output> {
    spawn_acdc(None, args, input)
}

#[cfg(any(feature = "html", feature = "lint"))]
fn run_acdc_in(
    working_dir: &std::path::Path,
    args: &[&str],
    input: Option<&str>,
) -> io::Result<Output> {
    spawn_acdc(Some(working_dir), args, input)
}

fn output_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(any(
    feature = "html",
    feature = "lint",
    feature = "inspect",
    feature = "execute"
))]
fn include_budget_commands() -> &'static [&'static [&'static str]] {
    &[
        #[cfg(feature = "html")]
        &["convert", "--backend", "html", "--out-file", "-"],
        #[cfg(feature = "lint")]
        &["lint", "--output-style", "compact"],
        #[cfg(feature = "inspect")]
        &["inspect"],
        #[cfg(feature = "execute")]
        &["execute", "--list"],
    ]
}

#[cfg(any(
    feature = "html",
    feature = "lint",
    feature = "inspect",
    feature = "execute"
))]
#[test]
fn include_budget_override_applies_to_each_document_command() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let entry = directory.path().join("entry.adoc");
    fs::write(directory.path().join("part.adoc"), "abc")?;
    fs::write(&entry, "include::part.adoc[]\ninclude::part.adoc[]\n")?;
    let entry = entry.to_str().ok_or("non-UTF-8 temporary path")?;
    for command in include_budget_commands() {
        for bytes in ["7", "8"] {
            let mut args = command.to_vec();
            args.extend(["--max-total-include-bytes", bytes, entry]);
            let output = run_acdc(&args, None)?;
            let stderr = output_text(&output.stderr);
            if bytes == "8" {
                assert!(output.status.success(), "{args:?}: {stderr}");
            } else {
                assert_eq!(output.status.code(), Some(1), "{args:?}: {stderr}");
                assert!(stderr.contains("limit of 7 bytes"), "{args:?}: {stderr}");
            }
        }
    }
    Ok(())
}

#[cfg(any(
    feature = "html",
    feature = "lint",
    feature = "inspect",
    feature = "execute"
))]
#[test]
fn include_budget_rejects_invalid_cli_values() -> Result<(), Box<dyn Error>> {
    for command in include_budget_commands() {
        for value in ["invalid", "-1", "18446744073709551616"] {
            let mut args = command.to_vec();
            args.extend(["--max-total-include-bytes", value, "unused.adoc"]);
            let output = run_acdc(&args, None)?;
            assert_eq!(
                output.status.code(),
                Some(2),
                "{args:?}: {}",
                output_text(&output.stderr)
            );
        }
    }
    Ok(())
}

#[cfg(any(feature = "html", feature = "lint"))]
#[test]
fn include_budget_override_applies_to_stdin() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    fs::write(directory.path().join("part.adoc"), "abc")?;
    let input = "include::part.adoc[]\ninclude::part.adoc[]\n";
    let commands: &[&[&str]] = &[
        #[cfg(feature = "html")]
        &["convert", "--out-file", "-"],
        #[cfg(feature = "lint")]
        &["lint"],
    ];
    for command in commands {
        for bytes in ["7", "8"] {
            let mut args = command.to_vec();
            args.extend(["--stdin", "--max-total-include-bytes", bytes]);
            let output = run_acdc_in(directory.path(), &args, Some(input))?;
            let stderr = output_text(&output.stderr);
            if bytes == "8" {
                assert!(output.status.success(), "{args:?}: {stderr}");
            } else {
                assert_eq!(output.status.code(), Some(1), "{args:?}: {stderr}");
                assert!(stderr.contains("limit of 7 bytes"), "{args:?}: {stderr}");
            }
        }
    }
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn include_budget_resets_for_each_file_in_a_conversion_batch() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    fs::write(directory.path().join("part.adoc"), "abc")?;
    for name in ["a", "b"] {
        fs::write(
            directory.path().join(format!("{name}.adoc")),
            "include::part.adoc[]\n",
        )?;
    }
    let output = run_acdc_in(
        directory.path(),
        &[
            "convert",
            "--max-total-include-bytes",
            "4",
            "a.adoc",
            "b.adoc",
        ],
        None,
    )?;
    assert!(output.status.success(), "{}", output_text(&output.stderr));
    for name in ["a", "b"] {
        assert!(
            fs::read_to_string(directory.path().join(format!("{name}.html")))?
                .contains("<p>abc</p>")
        );
    }
    Ok(())
}

#[cfg(not(any(
    feature = "html",
    feature = "manpage",
    feature = "markdown",
    feature = "pdf",
    feature = "terminal",
    feature = "execute",
    feature = "inspect",
    feature = "lint",
    feature = "tck",
)))]
#[test]
fn no_command_features_return_a_clear_diagnostic() -> Result<(), Box<dyn Error>> {
    let output = run_acdc(&[], None)?;
    let stderr = output_text(&output.stderr);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr.contains("built without any subcommand features"));
    assert!(stderr.contains("pdf"));
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn convert_requires_an_input() -> Result<(), Box<dyn Error>> {
    let output = run_acdc(&["convert"], None)?;
    let stderr = output_text(&output.stderr);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr.contains("required arguments were not provided"));
    assert!(
        stderr.contains(&format!(
            "Usage: acdc{} convert",
            std::env::consts::EXE_SUFFIX
        )),
        "{stderr}"
    );
    Ok(())
}

#[cfg(feature = "lint")]
#[test]
fn lint_requires_an_input() -> Result<(), Box<dyn Error>> {
    let output = run_acdc(&["lint"], None)?;
    let stderr = output_text(&output.stderr);

    assert_eq!(output.status.code(), Some(2));
    assert!(stderr.contains("required arguments were not provided"));
    assert!(
        stderr.contains(&format!("Usage: acdc{} lint", std::env::consts::EXE_SUFFIX)),
        "{stderr}"
    );
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn missing_input_file_returns_a_failure() -> Result<(), Box<dyn Error>> {
    let dir = tempdir()?;
    let missing_path = dir.path().join("missing.adoc");
    let missing = missing_path.to_str().ok_or("non-UTF-8 temporary path")?;
    let read_error = fs::read_to_string(missing)
        .err()
        .ok_or("missing input unexpectedly exists")?;
    let output = run_acdc(&["convert", missing], None)?;
    let stderr = output_text(&output.stderr);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains(missing));
    assert!(stderr.contains(&read_error.to_string()));
    Ok(())
}

#[cfg(feature = "lint")]
#[test]
fn denied_lint_returns_a_failure() -> Result<(), Box<dyn Error>> {
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

#[cfg(feature = "lint")]
#[test]
fn one_sentence_per_line_accepts_formatted_sentences() -> Result<(), Box<dyn Error>> {
    let output = run_acdc(
        &[
            "lint",
            "--stdin",
            "--output-style",
            "compact",
            "-A",
            "all",
            "-D",
            "one-sentence-per-line",
        ],
        Some(
            "= Title\n\n*One sentence.*\n_Another sentence._\n\n\
             * *One sentence.*\n  A second sentence.\n\n\
             *The supported values are:*\nUse `foo` for one mode.\n",
        ),
    )?;
    let stderr = output_text(&output.stderr);

    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    Ok(())
}

#[cfg(feature = "lint")]
#[test]
fn one_sentence_per_line_denies_formatted_violations() -> Result<(), Box<dyn Error>> {
    for (body, message) in [
        (
            "*One sentence.* *Another sentence.*\n",
            "multiple sentences on one source line",
        ),
        (
            "One sentence. *Another sentence*.\n",
            "multiple sentences on one source line",
        ),
        (
            "*One sentence wraps\nonto this line.*\n",
            "sentence spans multiple source lines",
        ),
    ] {
        let output = run_acdc(
            &[
                "lint",
                "--stdin",
                "--output-style",
                "compact",
                "-A",
                "all",
                "-D",
                "one-sentence-per-line",
            ],
            Some(&format!("= Title\n\n{body}")),
        )?;
        let stderr = output_text(&output.stderr);

        assert_eq!(output.status.code(), Some(1), "{body}: {stderr}");
        assert_eq!(stderr.matches("deny[one-sentence-per-line]").count(), 1);
        assert!(stderr.contains(&format!("at 3:1: {message}")), "{stderr}");
    }
    Ok(())
}

#[cfg(feature = "lint")]
#[test]
fn one_sentence_per_line_checks_description_list_values() -> Result<(), Box<dyn Error>> {
    for (body, expected_message) in [
        (
            "Subject:: One sentence. Another sentence.\n",
            Some("multiple sentences on one source line"),
        ),
        (
            "Subject:: *One sentence.* _Another sentence._\n",
            Some("multiple sentences on one source line"),
        ),
        (
            "Subject:: One sentence wraps\nonto another line.\n",
            Some("sentence spans multiple source lines"),
        ),
        ("Subject:: One sentence.\n", None),
        ("One term. Another:: One sentence.\n", None),
    ] {
        let output = run_acdc(
            &[
                "lint",
                "--stdin",
                "--output-style",
                "compact",
                "-A",
                "all",
                "-D",
                "one-sentence-per-line",
            ],
            Some(&format!("= Title\n\n{body}")),
        )?;
        let stderr = output_text(&output.stderr);

        if let Some(message) = expected_message {
            assert_eq!(output.status.code(), Some(1), "{body}: {stderr}");
            assert_eq!(stderr.matches("deny[one-sentence-per-line]").count(), 1);
            assert!(stderr.contains(&format!("at 3:1: {message}")), "{stderr}");
        } else {
            assert_eq!(output.status.code(), Some(0), "{body}: {stderr}");
            assert!(stderr.is_empty(), "{stderr}");
        }
    }
    Ok(())
}

#[cfg(feature = "tck")]
#[test]
fn invalid_tck_type_returns_a_failure() -> Result<(), Box<dyn Error>> {
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
fn converts_stdin_to_stdout() -> Result<(), Box<dyn Error>> {
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

#[cfg(any(feature = "html", feature = "terminal"))]
const WARNING_SOURCE: &str = "= Warnings\n\n== First\n\n==== Skipped\n\nRetained warning body.\n";
#[cfg(any(feature = "html", feature = "terminal"))]
const SECTION_WARNING: &str = "section title out of sequence";

#[cfg(feature = "html")]
#[test]
fn fail_on_warnings_handles_stdin_and_remains_independent_of_strict() -> Result<(), Box<dyn Error>>
{
    for flags in [
        vec![],
        vec!["--strict"],
        vec!["--fail-on-warnings"],
        vec!["--strict", "--fail-on-warnings"],
    ] {
        let mut args = vec!["convert", "--stdin", "--embedded", "--out-file", "-"];
        args.extend(&flags);
        let output = run_acdc(&args, Some(WARNING_SOURCE))?;
        let stderr = output_text(&output.stderr);
        let failed = flags.contains(&"--fail-on-warnings");

        assert_eq!(output.status.code(), Some(i32::from(failed)), "{stderr}");
        assert!(output_text(&output.stdout).contains("Retained warning body."));
        assert_eq!(stderr.matches(SECTION_WARNING).count(), 1, "{stderr}");
        assert_eq!(
            stderr.contains("conversion emitted 1 warning; --fail-on-warnings is set"),
            failed,
            "{stderr}"
        );
    }
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn fail_on_warnings_allows_clean_conversion() -> Result<(), Box<dyn Error>> {
    for flags in [
        vec!["--fail-on-warnings"],
        vec!["--strict", "--fail-on-warnings"],
    ] {
        let mut args = vec!["convert", "--stdin", "--embedded", "--out-file", "-"];
        args.extend(flags);
        let output = run_acdc(&args, Some("Clean conversion body.\n"))?;

        assert!(output.status.success(), "{}", output_text(&output.stderr));
        assert!(output_text(&output.stdout).contains("Clean conversion body."));
        assert!(output.stderr.is_empty(), "{}", output_text(&output.stderr));
    }
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn fail_on_warnings_retains_file_and_stdout_output_for_file_input() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let input = directory.path().join("warning.adoc");
    let explicit = directory.path().join("explicit.html");
    fs::write(&input, WARNING_SOURCE)?;
    let input_arg = input.to_string_lossy();
    let explicit_arg = explicit.to_string_lossy();

    for destination in [None, Some("-"), Some(explicit_arg.as_ref())] {
        let mut args = vec!["convert", "--fail-on-warnings", input_arg.as_ref()];
        if let Some(destination) = destination {
            args.extend(["--out-file", destination]);
        }
        let output = run_acdc(&args, None)?;
        let stderr = output_text(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{stderr}");
        assert_eq!(stderr.matches(SECTION_WARNING).count(), 1, "{stderr}");
        let rendered = match destination {
            Some("-") => output_text(&output.stdout),
            Some(_) => fs::read_to_string(&explicit)?,
            None => fs::read_to_string(input.with_extension("html"))?,
        };
        assert!(rendered.contains("Retained warning body."));
    }
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn fail_on_warnings_counts_preprocessor_warnings_once() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let input = "include::missing-sc-272.adoc[]\n\nPreprocessor body retained.\n";
    for fail_on_warnings in [false, true] {
        let mut args = vec!["convert", "--stdin", "--embedded", "--out-file", "-"];
        if fail_on_warnings {
            args.push("--fail-on-warnings");
        }
        let output = run_acdc_in(directory.path(), &args, Some(input))?;
        let stderr = output_text(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(i32::from(fail_on_warnings)),
            "{stderr}"
        );
        assert_eq!(
            stderr.matches("include file not found").count(),
            1,
            "{stderr}"
        );
        assert!(output_text(&output.stdout).contains("Preprocessor body retained."));
        assert_eq!(
            stderr.contains("conversion emitted 1 warning"),
            fail_on_warnings,
            "{stderr}"
        );
    }
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn fail_on_warnings_counts_parser_and_converter_warnings_once() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let input = directory.path().join("styles.adoc");
    let source = "= Styles\n:stylesheet: missing-sc-272.css\n\n== First\n\n==== Skipped\n\nStylesheet body retained.\n";
    fs::write(&input, source)?;
    let input_arg = input.to_string_lossy();
    for from_stdin in [false, true] {
        for fail_on_warnings in [false, true] {
            let mut args = vec!["convert", "--out-file", "-"];
            args.push(if from_stdin {
                "--stdin"
            } else {
                input_arg.as_ref()
            });
            if fail_on_warnings {
                args.push("--fail-on-warnings");
            }
            let output = run_acdc_in(directory.path(), &args, from_stdin.then_some(source))?;
            let stderr = output_text(&output.stderr);
            assert_eq!(
                output.status.code(),
                Some(i32::from(fail_on_warnings)),
                "{stderr}"
            );
            assert_eq!(stderr.matches(SECTION_WARNING).count(), 1, "{stderr}");
            assert_eq!(
                stderr.matches("could not read custom stylesheet").count(),
                1,
                "{stderr}"
            );
            assert!(output_text(&output.stdout).contains("Stylesheet body retained."));
            assert_eq!(
                stderr.contains("conversion emitted 2 warnings"),
                fail_on_warnings,
                "{stderr}"
            );
        }
    }
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn fail_on_warnings_processes_all_selected_files_and_preserves_error_reporting()
-> Result<(), Box<dyn Error>> {
    for include_error in [false, true] {
        let directory = tempdir()?;
        let warning = directory.path().join("warning.adoc");
        let clean = directory.path().join("clean.adoc");
        let missing = directory.path().join("missing.adoc");
        fs::write(&warning, WARNING_SOURCE)?;
        fs::write(&clean, "Clean file body.\n")?;
        let warning_arg = warning.to_string_lossy();
        let clean_arg = clean.to_string_lossy();
        let missing_arg = missing.to_string_lossy();
        let mut args = vec!["convert", "--fail-on-warnings", "--timings"];
        if include_error {
            args.push(missing_arg.as_ref());
        }
        args.extend([warning_arg.as_ref(), clean_arg.as_ref()]);

        let output = run_acdc(&args, None)?;
        let stderr = output_text(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{stderr}");
        assert_eq!(stderr.matches(SECTION_WARNING).count(), 1, "{stderr}");
        assert!(
            fs::read_to_string(warning.with_extension("html"))?.contains("Retained warning body.")
        );
        assert!(fs::read_to_string(clean.with_extension("html"))?.contains("Clean file body."));
        assert_eq!(
            stderr.contains("failed to process 1 file(s)"),
            include_error,
            "{stderr}"
        );
        assert_eq!(
            stderr.contains("conversion emitted 1 warning"),
            !include_error,
            "{stderr}"
        );
        assert!(stderr.contains("Wall clock"), "{stderr}");
    }
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn fail_on_warnings_keeps_explicit_output_first_file_policy() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let warning = directory.path().join("warning.adoc");
    let clean = directory.path().join("clean.adoc");
    let destination = directory.path().join("result.html");
    fs::write(&warning, WARNING_SOURCE)?;
    fs::write(&clean, "Clean file body.\n")?;
    let warning_arg = warning.to_string_lossy();
    let clean_arg = clean.to_string_lossy();
    let destination_arg = destination.to_string_lossy();
    for warning_first in [false, true] {
        let files = if warning_first {
            [warning_arg.as_ref(), clean_arg.as_ref()]
        } else {
            [clean_arg.as_ref(), warning_arg.as_ref()]
        };
        let mut args = vec![
            "convert",
            "--fail-on-warnings",
            "--out-file",
            destination_arg.as_ref(),
        ];
        args.extend(files);
        let output = run_acdc(&args, None)?;
        let stderr = output_text(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(i32::from(warning_first)),
            "{stderr}"
        );
        assert_eq!(
            stderr.matches(SECTION_WARNING).count(),
            usize::from(warning_first),
            "{stderr}"
        );
        assert!(stderr.contains("only processing first file"), "{stderr}");
        let rendered = fs::read_to_string(&destination)?;
        assert_eq!(rendered.contains("Retained warning body."), warning_first);
        assert_eq!(rendered.contains("Clean file body."), !warning_first);
    }
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn fail_on_warnings_ignores_tracing_and_cli_notices() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let input = directory.path().join("clean.adoc");
    fs::write(&input, "Clean file body.\n")?;
    let output = Command::new(env!("CARGO_BIN_EXE_acdc"))
        .args(["convert", "--fail-on-warnings", "--open", "--out-file", "-"])
        .arg(&input)
        .env("ACDC_LOG", "warn")
        .output()?;
    let stderr = output_text(&output.stderr);

    assert!(output.status.success(), "{stderr}");
    assert_eq!(
        stderr
            .matches("--open ignored when output is stdout")
            .count(),
        2,
        "{stderr}"
    );
    assert!(output_text(&output.stdout).contains("Clean file body."));
    Ok(())
}

#[cfg(feature = "terminal")]
#[test]
fn fail_on_warnings_covers_terminal_stdin_files_and_outputs() -> Result<(), Box<dyn Error>> {
    let directory = tempdir()?;
    let input = directory.path().join("warning.adoc");
    let destination = directory.path().join("result.txt");
    let source =
        format!("{WARNING_SOURCE}\n[source,php,options=mixed]\n----\n<?php echo 1; ?>\n----\n");
    fs::write(&input, &source)?;
    let input_arg = input.to_string_lossy();
    let destination_arg = destination.to_string_lossy();
    for from_stdin in [false, true] {
        for to_file in [false, true] {
            let mut args = vec![
                "convert",
                "--backend",
                "terminal",
                "--no-pager",
                "--fail-on-warnings",
            ];
            args.push(if from_stdin {
                "--stdin"
            } else {
                input_arg.as_ref()
            });
            if to_file {
                args.extend(["--out-file", destination_arg.as_ref()]);
            }
            let output = run_acdc(&args, from_stdin.then_some(source.as_str()))?;
            let stderr = output_text(&output.stderr);
            assert_eq!(output.status.code(), Some(1), "{stderr}");
            assert_eq!(stderr.matches(SECTION_WARNING).count(), 1, "{stderr}");
            assert_eq!(
                stderr
                    .matches("PHP source block mixed-mode highlighting")
                    .count(),
                1,
                "{stderr}"
            );
            assert!(stderr.contains("conversion emitted 2 warnings"), "{stderr}");
            let rendered = if to_file {
                fs::read_to_string(&destination)?
            } else {
                output_text(&output.stdout)
            };
            assert!(rendered.contains("Retained warning body."));
        }
    }

    let clean = directory.path().join("clean.adoc");
    fs::write(&clean, "Clean terminal body.\n")?;
    let clean_arg = clean.to_string_lossy();
    let output = run_acdc(
        &[
            "convert",
            "--backend",
            "terminal",
            "--no-pager",
            "--fail-on-warnings",
            input_arg.as_ref(),
            clean_arg.as_ref(),
        ],
        None,
    )?;
    let stderr = output_text(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("conversion emitted 2 warnings"), "{stderr}");
    assert!(output_text(&output.stdout).contains("Clean terminal body."));
    Ok(())
}

#[cfg(feature = "pdf")]
#[test]
fn fail_on_warnings_keeps_strict_asset_errors_fatal() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let input = directory.path().join("assets.adoc");
    std::fs::write(&input, "= Assets\n\nimage::missing-sc-272.svg[]\n")?;
    let input_arg = input.to_string_lossy();
    for strict in [false, true] {
        for fail_on_warnings in [false, true] {
            let mut args = vec![
                "convert",
                "--backend",
                "pdf",
                "--out-file",
                "-",
                input_arg.as_ref(),
            ];
            if strict {
                args.push("--strict");
            }
            if fail_on_warnings {
                args.push("--fail-on-warnings");
            }
            let output = run_acdc(&args, None)?;
            let stderr = output_text(&output.stderr);

            assert_eq!(
                output.status.code(),
                Some(i32::from(strict || fail_on_warnings)),
                "{stderr}"
            );
            if strict {
                assert_eq!(output.stdout, [] as [u8; 0]);
                assert!(stderr.contains("failed to resolve 1 image(s)"), "{stderr}");
                assert!(!stderr.contains("conversion emitted"), "{stderr}");
            } else {
                assert!(output.stdout.starts_with(b"%PDF-"));
                assert_eq!(
                    stderr.contains("conversion emitted 1 warning"),
                    fail_on_warnings,
                    "{stderr}"
                );
            }
        }
    }
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn command_line_attributes_cannot_be_changed_by_document_entries() -> Result<(), Box<dyn Error>> {
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
fn soft_command_line_attributes_can_be_changed_by_document_entries() -> Result<(), Box<dyn Error>> {
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
fn converter_defaults_remain_document_overridable() -> Result<(), Box<dyn Error>> {
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
fn implied_and_conversion_only_attributes_are_not_available_in_the_parser()
-> Result<(), Box<dyn Error>> {
    let output = run_acdc(
        &[
            "convert",
            "--stdin",
            "--out-file",
            "-",
            "-a",
            "outdir=caller-dir",
            "-a",
            "outfile=caller-file",
        ],
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
fn selected_backend_attributes_are_available_during_parsing() -> Result<(), Box<dyn Error>> {
    let temp = tempdir()?;
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
fn converts_multiple_files_with_a_timing_summary() -> Result<(), Box<dyn Error>> {
    let temp = tempdir()?;
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
fn terminal_converts_multiple_files_without_a_pager() -> Result<(), Box<dyn Error>> {
    let temp = tempdir()?;
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
fn all_backends_render_recovered_bibliography_children() -> Result<(), Box<dyn Error>> {
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

    let temp = tempdir()?;
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
fn inspect_resolves_includes_and_omits_ansi_when_piped() -> Result<(), Box<dyn Error>> {
    let temp = tempdir()?;
    let included = temp.path().join("included.adoc");
    let document = temp.path().join("document.adoc");
    fs::write(&included, "Included paragraph.\n")?;
    fs::write(&document, "= Document\n\ninclude::included.adoc[]\n")?;
    let document_arg = document.to_string_lossy();

    let output = run_acdc(&["inspect", document_arg.as_ref()], None)?;
    let stdout = output_text(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("Included paragraph."));
    assert!(!stdout.contains("max-include-depth"));
    assert!(!stdout.contains('\u{1b}'));
    Ok(())
}

#[cfg(feature = "execute")]
#[test]
fn execute_dry_run_prints_the_plan_in_dependency_order() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let document = temp.path().join("commands.adoc");
    fs::write(
        &document,
        "[.command, id=test, deps=\"build\"]\n----\necho testing\n----\n\n\
         [.command, id=build]\n[source, bash]\n----\necho building\n----\n",
    )?;
    let document_arg = document.to_string_lossy();

    let output = run_acdc(&["execute", "--dry-run", document_arg.as_ref()], None)?;
    let stdout = output_text(&output.stdout);

    assert!(output.status.success());
    let build = stdout.find("build (bash)").ok_or("build plan missing")?;
    let test = stdout.find("test (sh)").ok_or("test plan missing")?;
    assert!(build < test, "dependency must print first: {stdout}");
    assert!(stdout.contains("  echo testing"));
    Ok(())
}

#[cfg(all(feature = "execute", unix))]
#[test]
fn execute_runs_selected_commands() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let marker = temp.path().join("ran");
    let document = temp.path().join("commands.adoc");
    fs::write(
        &document,
        format!(
            "[.command, id=touch-marker]\n----\necho run >> {}\n----\n",
            marker.display()
        ),
    )?;
    let document_arg = document.to_string_lossy();

    let output = run_acdc(
        &["execute", "--id", "touch-marker", document_arg.as_ref()],
        None,
    )?;

    assert!(output.status.success());
    assert!(marker.exists());
    Ok(())
}

#[cfg(all(feature = "execute", unix))]
#[test]
fn execute_failure_returns_a_failing_exit() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let document = temp.path().join("commands.adoc");
    fs::write(&document, "[.command, id=fail]\n----\nexit 7\n----\n")?;
    let document_arg = document.to_string_lossy();

    let output = run_acdc(&["execute", document_arg.as_ref()], None)?;
    let stderr = output_text(&output.stderr);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("`fail` exited with"));
    Ok(())
}

#[cfg(all(feature = "execute", unix))]
mod execution {
    use std::path::Path;

    use super::*;

    fn run_document(
        directory: &Path,
        source: &str,
        flags: &[&str],
    ) -> Result<Output, Box<dyn Error>> {
        let document = directory.join("commands.adoc");
        fs::write(&document, source)?;
        Ok(Command::new(env!("CARGO_BIN_EXE_acdc"))
            .arg("execute")
            .arg(&document)
            .args(flags)
            .current_dir(directory)
            .env("ACDC_EXECUTE_INHERITED", "inherited")
            .output()?)
    }

    #[test]
    fn execute_blocks_transitive_dependents_but_runs_independent_commands()
    -> Result<(), Box<dyn Error>> {
        let directory = tempfile::tempdir()?;
        let output = run_document(
            directory.path(),
            concat!(
                "[.command,id=build]\n----\nexit 7\n----\n\n",
                "[.command,id=test,deps=build]\n----\ntouch test\n----\n\n",
                "[.command,id=deploy,deps=test]\n----\ntouch deploy\n----\n\n",
                "[.command,id=independent]\n----\ntouch independent\n----\n",
            ),
            &[],
        )?;
        assert_eq!(output.status.code(), Some(1));
        let stderr = output_text(&output.stderr);
        assert!(stderr.contains("command `build` exited with"), "{stderr}");
        assert!(
            stderr.contains("prerequisite `build` did not succeed"),
            "{stderr}"
        );
        assert!(
            stderr.contains("prerequisite `test` did not succeed"),
            "{stderr}"
        );
        assert!(!directory.path().join("test").exists());
        assert!(!directory.path().join("deploy").exists());
        assert!(directory.path().join("independent").exists());
        Ok(())
    }

    #[test]
    fn execute_exit_on_failure_stops_independent_commands() -> Result<(), Box<dyn Error>> {
        let directory = tempfile::tempdir()?;
        let output = run_document(
            directory.path(),
            concat!(
                "[.command,id=bad]\n----\nexit 7\n----\n\n",
                "[.command,id=after]\n----\ntouch after\n----\n",
            ),
            &["--exit-on-failure"],
        )?;
        assert_eq!(output.status.code(), Some(1));
        assert!(!directory.path().join("after").exists());
        assert!(output_text(&output.stderr).contains("skipped after failure of `bad`"));
        Ok(())
    }

    #[test]
    fn execute_cwd_and_env_override_only_child_settings() -> Result<(), Box<dyn Error>> {
        let directory = tempfile::tempdir()?;
        fs::create_dir(directory.path().join("child"))?;
        let output = run_document(
            directory.path(),
            concat!(
                "[.command,id=child]\n----\n",
                "printf '%s|%s' \"$ACDC_EXECUTE_INHERITED\" \"$ACDC_EXECUTE_VALUE\" > value\n----\n",
            ),
            &[
                "--cwd",
                "child",
                "--env",
                "ACDC_EXECUTE_VALUE=first",
                "--env",
                "ACDC_EXECUTE_VALUE=last=kept",
            ],
        )?;
        assert!(output.status.success(), "{}", output_text(&output.stderr));
        assert_eq!(
            fs::read_to_string(directory.path().join("child/value"))?,
            "inherited|last=kept"
        );
        assert!(!directory.path().join("value").exists());
        Ok(())
    }

    #[test]
    fn execute_defaults_inherit_caller_directory_and_environment() -> Result<(), Box<dyn Error>> {
        let directory = tempfile::tempdir()?;
        let output = run_document(
            directory.path(),
            "[.command,id=defaults]\n----\nprintf '%s' \"$ACDC_EXECUTE_INHERITED\" > inherited\n----\n",
            &[],
        )?;
        assert!(output.status.success(), "{}", output_text(&output.stderr));
        assert_eq!(
            fs::read_to_string(directory.path().join("inherited"))?,
            "inherited"
        );
        Ok(())
    }

    #[test]
    fn execute_list_includes_prerequisites_and_descriptions_without_running()
    -> Result<(), Box<dyn Error>> {
        let directory = tempfile::tempdir()?;
        let output = run_document(
            directory.path(),
            concat!(
                "= Commands\n\n",
                "[.command,id=build,description=Compile]\n----\ntouch build\n----\n\n",
                "== Tests\n\n",
                "[source,bash,role=command,id=test,deps=build,interpreter=sh]\n",
                "----\ntouch test\n----\n",
            ),
            &["--list", "--id", "test"],
        )?;
        assert!(output.status.success(), "{}", output_text(&output.stderr));
        assert_eq!(
            output_text(&output.stdout),
            concat!(
                "- id=build, interpreter=\"sh\", description=\"Compile\"\n",
                "- id=test, interpreter=\"sh\", section=\"Tests\", deps=\"build\"\n",
            )
        );
        assert!(!directory.path().join("build").exists());
        assert!(!directory.path().join("test").exists());
        Ok(())
    }

    #[test]
    fn execute_rejects_recovered_source_before_any_child_starts() -> Result<(), Box<dyn Error>> {
        for source in [
            "[.command,id=incomplete]\n----\ntouch marker\n",
            "[.command,id=missing]\n----\ninclude::missing.sh[]\ntouch marker\n----\n",
        ] {
            let directory = tempfile::tempdir()?;
            let output = run_document(directory.path(), source, &[])?;
            assert_eq!(
                output.status.code(),
                Some(1),
                "{}",
                output_text(&output.stderr)
            );
            assert!(!directory.path().join("marker").exists());
        }
        Ok(())
    }

    #[test]
    fn execute_preserves_callout_text_in_heredocs() -> Result<(), Box<dyn Error>> {
        let directory = tempfile::tempdir()?;
        let output = run_document(
            directory.path(),
            "[.command,id=literal]\n----\ncat <<'EOF'\n<1>\nEOF\n----\n",
            &[],
        )?;
        assert!(output.status.success(), "{}", output_text(&output.stderr));
        assert_eq!(output.stdout, b"<1>\n");
        Ok(())
    }

    #[test]
    fn execute_keeps_attribute_references_literal_by_default() -> Result<(), Box<dyn Error>> {
        let directory = tempfile::tempdir()?;
        let source = concat!(
            "= Commands\n:value: expanded\n\n",
            "[.command,id=literal]\n----\n",
            "printf '%s' '{value}|\\{value}|<1>'\n----\n",
        );
        let dry_run = run_document(directory.path(), source, &["--dry-run"])?;
        assert!(dry_run.status.success(), "{}", output_text(&dry_run.stderr));
        assert_eq!(
            output_text(&dry_run.stdout),
            "literal (sh)\n  printf '%s' '{value}|\\{value}|<1>'\n"
        );
        let executed = run_document(directory.path(), source, &[])?;
        assert!(
            executed.status.success(),
            "{}",
            output_text(&executed.stderr)
        );
        assert_eq!(executed.stdout, b"{value}|\\{value}|<1>");
        Ok(())
    }

    #[cfg(feature = "pre-spec-subs")]
    #[test]
    fn execute_dry_run_and_interpreter_use_the_same_attribute_substitutions()
    -> Result<(), Box<dyn Error>> {
        for substitutions in ["attributes", "+attributes", "attributes+", "normal"] {
            let directory = tempfile::tempdir()?;
            let source = format!(
                "= Commands\n:value: hello <1>\n:empty:\n\n\
                 [.command,id=expanded,subs=\"{substitutions}\"]\n----\n\
                 printf '%s|%s|%s|%s' '{{value}}' '\\{{missing}}' '{{empty}}' '<2>'\n----\n"
            );
            let dry_run = run_document(directory.path(), &source, &["--dry-run"])?;
            assert!(
                dry_run.status.success(),
                "{substitutions}: {}",
                output_text(&dry_run.stderr)
            );
            assert_eq!(
                output_text(&dry_run.stdout),
                "expanded (sh)\n  printf '%s|%s|%s|%s' 'hello <1>' '{missing}' '' '<2>'\n",
                "{substitutions}"
            );
            let executed = run_document(directory.path(), &source, &[])?;
            assert!(
                executed.status.success(),
                "{substitutions}: {}",
                output_text(&executed.stderr)
            );
            assert_eq!(
                executed.stdout, b"hello <1>|{missing}||<2>",
                "{substitutions}"
            );
        }
        Ok(())
    }

    #[cfg(feature = "pre-spec-subs")]
    #[test]
    fn execute_keeps_disabled_attribute_substitutions_literal() -> Result<(), Box<dyn Error>> {
        for substitutions in ["none", "-attributes"] {
            let directory = tempfile::tempdir()?;
            let source = format!(
                "= Commands\n:value: expanded\n\n\
                 [.command,id=literal,subs=\"{substitutions}\"]\n----\n\
                 printf '%s' '{{value}}|\\{{value}}|<1>'\n----\n"
            );
            let output = run_document(directory.path(), &source, &[])?;
            assert!(
                output.status.success(),
                "{substitutions}: {}",
                output_text(&output.stderr)
            );
            assert_eq!(output.stdout, b"{value}|\\{value}|<1>", "{substitutions}");
        }
        Ok(())
    }

    #[cfg(feature = "pre-spec-subs")]
    #[test]
    fn execute_rejects_missing_attributes_in_unselected_commands_before_starting()
    -> Result<(), Box<dyn Error>> {
        for flags in [vec!["--id", "first"], vec!["--id", "first", "--dry-run"]] {
            let directory = tempfile::tempdir()?;
            let output = run_document(
                directory.path(),
                concat!(
                    "[.command,id=first]\n----\nprintf ran > marker\n----\n\n",
                    "[.command,id=broken,subs=attributes]\n----\nprintf '%s' '{missing}'\n----\n",
                ),
                &flags,
            )?;
            assert_eq!(output.status.code(), Some(1));
            let stderr = output_text(&output.stderr);
            assert!(
                stderr.contains("command `broken` references missing document attribute `missing`"),
                "{stderr}"
            );
            assert!(stderr.contains("commands.adoc"), "{stderr}");
            assert!(!directory.path().join("marker").exists());
        }
        Ok(())
    }

    #[cfg(feature = "pre-spec-subs")]
    #[test]
    fn execute_attribute_substitutions_follow_source_order() -> Result<(), Box<dyn Error>> {
        let directory = tempfile::tempdir()?;
        let source = concat!(
            "= Commands\n:value: first\n\n",
            "[.command,id=first,subs=attributes]\n----\nprintf '%s\\n' '{value}'\n----\n\n",
            ":value: second\n\n",
            "[.command,id=second,subs=attributes]\n----\nprintf '%s\\n' '{value}'\n----\n",
        );
        let dry_run = run_document(directory.path(), source, &["--dry-run"])?;
        assert!(dry_run.status.success(), "{}", output_text(&dry_run.stderr));
        assert_eq!(
            output_text(&dry_run.stdout),
            concat!(
                "first (sh)\n  printf '%s\\n' 'first'\n",
                "second (sh)\n  printf '%s\\n' 'second'\n",
            )
        );
        let output = run_document(directory.path(), source, &[])?;
        assert!(output.status.success(), "{}", output_text(&output.stderr));
        assert_eq!(output.stdout, b"first\nsecond\n");
        Ok(())
    }

    #[cfg(not(feature = "pre-spec-subs"))]
    #[test]
    fn execute_without_substitution_support_rejects_explicit_subs_before_starting()
    -> Result<(), Box<dyn Error>> {
        for substitutions in ["attributes", "none"] {
            let directory = tempfile::tempdir()?;
            let source = format!(
                "[.command,id=first]\n----\nprintf ran > marker\n----\n\n\
                 [.command,id=unsupported,subs={substitutions}]\n----\nprintf '%s' '{{value}}'\n----\n"
            );
            let output = run_document(directory.path(), &source, &["--id", "first"])?;
            assert_eq!(output.status.code(), Some(1));
            let stderr = output_text(&output.stderr);
            assert!(stderr.contains("subs"), "{stderr}");
            assert!(stderr.contains("commands.adoc"), "{stderr}");
            assert!(!directory.path().join("marker").exists());
        }
        Ok(())
    }

    #[test]
    fn execute_reports_the_resolved_included_file_for_invalid_commands()
    -> Result<(), Box<dyn Error>> {
        let directory = tempfile::tempdir()?;
        fs::create_dir(directory.path().join("nested"))?;
        let included = directory.path().join("nested/broken.adoc");
        fs::write(&included, "[.command]\n----\ntrue\n----\n")?;
        let output = run_document(directory.path(), "include::nested/broken.adoc[]\n", &[])?;
        assert_eq!(output.status.code(), Some(1));
        let stderr = output_text(&output.stderr);
        assert!(stderr.contains("missing an id"), "{stderr}");
        assert!(
            stderr.contains(included.to_string_lossy().as_ref()),
            "{stderr}"
        );
        assert!(stderr.contains("[.command]"), "{stderr}");
        Ok(())
    }

    #[test]
    fn execute_keeps_both_duplicate_declarations_in_diagnostics() -> Result<(), Box<dyn Error>> {
        let directory = tempfile::tempdir()?;
        fs::write(
            directory.path().join("first.adoc"),
            "[.command,id=duplicate]\n----\ntrue\n----\n",
        )?;
        fs::write(
            directory.path().join("second.adoc"),
            "[.command,id=duplicate]\n----\ntrue\n----\n",
        )?;
        let output = run_document(
            directory.path(),
            "include::first.adoc[]\n\ninclude::second.adoc[]\n",
            &[],
        )?;
        assert_eq!(output.status.code(), Some(1));
        let stderr = output_text(&output.stderr);
        assert!(stderr.contains("duplicate command id"), "{stderr}");
        assert!(stderr.contains("first.adoc"), "{stderr}");
        assert!(stderr.contains("second.adoc"), "{stderr}");
        Ok(())
    }
}

#[cfg(feature = "execute")]
#[test]
fn execute_reports_unknown_and_duplicate_selectors_as_diagnostics()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let document = temp.path().join("commands.adoc");
    fs::write(&document, "[.command, id=build]\n----\ntrue\n----\n")?;
    let document_arg = document.to_string_lossy();

    let unknown = run_acdc(&["execute", "--id", "missing", document_arg.as_ref()], None)?;
    assert_eq!(unknown.status.code(), Some(1));
    assert!(output_text(&unknown.stderr).contains("unknown command id: missing"));

    let no_match = run_acdc(
        &["execute", "--id-regex", "^zzz", document_arg.as_ref()],
        None,
    )?;
    assert_eq!(no_match.status.code(), Some(1));
    assert!(output_text(&no_match.stderr).contains("matched no commands"));
    Ok(())
}

/// An image in an included file must reach the parsed document for linting.
#[cfg(feature = "lint")]
#[test]
fn lint_stdin_resolves_includes_against_the_working_directory() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    fs::write(temp.path().join("part.adoc"), "image::photo.png[]\n")?;

    let output = run_acdc_in(
        temp.path(),
        &[
            "lint",
            "--stdin",
            "--output-style",
            "compact",
            "--deny",
            "image-alt-text",
        ],
        Some("= Lint stdin\n\ninclude::part.adoc[]\n"),
    )?;
    let stderr = output_text(&output.stderr);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr.contains("deny[image-alt-text]"),
        "expected the included file's image to be linted, got: {stderr}"
    );
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn stdin_resolves_includes_against_the_working_directory() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    fs::write(temp.path().join("part.adoc"), "Included body text.\n")?;

    let output = run_acdc_in(
        temp.path(),
        &["convert", "--stdin", "--out-file", "-"],
        Some("= Stdin include\n\ninclude::part.adoc[]\n"),
    )?;
    let stdout = output_text(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("Included body text."));
    assert!(!stdout.contains("include::part.adoc[]"));
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn secure_stdin_does_not_expand_includes() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    fs::write(temp.path().join("part.adoc"), "PRIVATE CONTENT")?;
    let output = run_acdc_in(
        temp.path(),
        &[
            "convert",
            "--stdin",
            "--safe-mode",
            "secure",
            "--out-file",
            "-",
        ],
        Some("include::part.adoc[]"),
    )?;
    assert!(output.status.success());
    let stdout = output_text(&output.stdout);
    assert!(!stdout.contains("PRIVATE CONTENT"));
    assert!(stdout.contains("href=\"part.adoc\""));
    Ok(())
}

#[cfg(feature = "lint")]
#[test]
fn secure_lint_stdin_does_not_expand_includes() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    fs::write(temp.path().join("part.adoc"), "image::photo.png[]\n")?;
    let output = run_acdc_in(
        temp.path(),
        &[
            "lint",
            "--stdin",
            "--safe-mode",
            "secure",
            "--deny",
            "image-alt-text",
        ],
        Some("include::part.adoc[]"),
    )?;
    assert!(output.status.success(), "{}", output_text(&output.stderr));
    assert!(!output_text(&output.stderr).contains("deny[image-alt-text]"));
    Ok(())
}

#[cfg(feature = "inspect")]
#[test]
fn inspect_include_access_is_controlled_by_safe_mode() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    fs::write(temp.path().join("part.adoc"), "PRIVATE CONTENT")?;
    let entry = temp.path().join("main.adoc");
    fs::write(&entry, "include::part.adoc[]")?;
    let entry = entry.to_str().ok_or("invalid entry path")?;
    let default = run_acdc(&["inspect", entry], None)?;
    let secure = run_acdc(&["inspect", "--safe-mode", "secure", entry], None)?;
    assert!(default.status.success());
    assert!(secure.status.success());
    assert!(output_text(&default.stdout).contains("PRIVATE CONTENT"));
    assert!(!output_text(&secure.stdout).contains("PRIVATE CONTENT"));
    Ok(())
}
