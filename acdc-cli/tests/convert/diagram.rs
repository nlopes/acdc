use super::{output_text, run_acdc_in};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;
const SOURCE: &str = ":imagesdir: images\n\n[wavedrom,clock,svg]\n....\nwave body\n....\n";

#[test]
fn diagrams_require_explicit_activation() -> TestResult {
    let dir = tempfile::tempdir()?;
    let output = run_acdc_in(
        dir.path(),
        &["convert", "--stdin", "--embedded"],
        Some(SOURCE),
    )?;
    assert!(output.status.success(), "{}", output_text(&output.stderr));
    assert!(output_text(&output.stdout).contains("wave body"));
    assert!(!dir.path().join("images").exists());
    Ok(())
}

#[cfg(not(feature = "diagram"))]
#[test]
fn diagram_is_unavailable_before_input_is_read() -> TestResult {
    let dir = tempfile::tempdir()?;
    let output = run_acdc_in(
        dir.path(),
        &["convert", "--extension", "diagram", "missing.adoc"],
        None,
    )?;
    assert!(!output.status.success());
    assert!(output_text(&output.stderr).contains("extension `diagram` is not available"));
    Ok(())
}

#[cfg(feature = "diagram")]
#[test]
fn diagram_configuration_requires_activation_and_tools() -> TestResult {
    let dir = tempfile::tempdir()?;
    for (args, expected) in [
        (vec!["--extension", "diagram"], "requires --wavedrom-cli"),
        (
            vec!["--wavedrom-cli", "missing"],
            "require --extension diagram",
        ),
        (
            vec!["--extension", "diagram", "--pdf2svg", "missing"],
            "--lualatex",
        ),
    ] {
        let mut command = vec!["convert", "missing.adoc"];
        command.extend(args);
        let output = run_acdc_in(dir.path(), &command, None)?;
        assert!(!output.status.success());
        assert!(
            output_text(&output.stderr).contains(expected),
            "{}",
            output_text(&output.stderr)
        );
    }
    Ok(())
}

#[cfg(all(feature = "diagram", unix))]
mod active {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Output};

    fn setup(root: &Path) -> TestResult {
        fs::write(
            root.join("renderer"),
            "#!/bin/sh\nprintf '%s' '<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><rect width=\"10\" height=\"10\"/></svg>' > \"$4\"\n",
        )?;
        fs::set_permissions(root.join("renderer"), fs::Permissions::from_mode(0o700))?;
        Ok(())
    }

    fn convert(root: &Path, args: &[&str], stdin: Option<&str>) -> std::io::Result<Output> {
        let mut command = vec![
            "convert",
            "--extension",
            "diagram",
            "--wavedrom-cli",
            "./renderer",
            "--fail-on-warnings",
        ];
        command.extend(args);
        run_acdc_in(root, &command, stdin)
    }

    #[test]
    fn titled_diagram_listing_shares_the_figure_counter_with_authored_images() -> TestResult {
        let dir = tempfile::tempdir()?;
        setup(dir.path())?;
        let source = ".Timing\n[wavedrom,clock,svg]\n----\nwave body\n----\n\n.Photo\nimage::photo.png[]\n\n----\nordinary listing\n----";
        let output = convert(dir.path(), &["--stdin", "--embedded"], Some(source))?;
        assert!(output.status.success(), "{}", output_text(&output.stderr));
        let html = output_text(&output.stdout);
        assert!(html.contains("src=\"clock.svg\""));
        assert!(html.contains("Figure 1. Timing") && html.contains("Figure 2. Photo"));
        assert!(html.contains("<pre>ordinary listing</pre>"));
        assert!(dir.path().join("clock.svg").is_file());
        Ok(())
    }

    #[test]
    fn failed_diagram_in_an_include_keeps_source_and_reports_a_warning() -> TestResult {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        setup(root)?;
        fs::write(root.join("renderer"), "#!/bin/sh\nexit 1\n")?;
        fs::write(root.join("clock.svg"), "previous asset")?;
        fs::write(
            root.join("guide.adoc"),
            ":label: clock\n\ninclude::part.adoc[]\n\nAfter.",
        )?;
        fs::write(
            root.join("part.adoc"),
            "\n[wavedrom,clock,svg,subs=attributes]\n....\n<{label}> *raw*\n....",
        )?;
        for strict in [false, true] {
            let mut args = vec![
                "convert",
                "--extension",
                "diagram",
                "--wavedrom-cli",
                "./renderer",
                "--embedded",
                "-o",
                "-",
                "guide.adoc",
            ];
            if strict {
                args.push("--fail-on-warnings");
            }
            let output = run_acdc_in(root, &args, None)?;
            let html = output_text(&output.stdout);
            let stderr = output_text(&output.stderr);
            assert_eq!(output.status.code(), Some(i32::from(strict)), "{stderr}");
            assert!(
                stderr.contains("renderer exited") && stderr.contains("part.adoc"),
                "{stderr}"
            );
            assert!(
                html.contains("Failed to generate image:") && html.contains("&lt;clock&gt; *raw*"),
                "{html}"
            );
            assert!(
                html.contains("<p>After.</p>") && !html.contains("src=\"clock.svg\""),
                "{html}"
            );
        }
        assert_eq!(
            fs::read_to_string(root.join("clock.svg"))?,
            "previous asset"
        );
        Ok(())
    }

    #[test]
    fn diagram_assets_follow_files_streams_and_batches() -> TestResult {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        setup(root)?;
        for folder in ["first", "second", "output"] {
            fs::create_dir(root.join(folder))?;
        }
        for folder in ["first", "second"] {
            fs::write(root.join(folder).join("doc.adoc"), SOURCE)?;
        }
        let batch = convert(root, &["first/doc.adoc", "second/doc.adoc"], None)?;
        assert!(batch.status.success(), "{}", output_text(&batch.stderr));
        for folder in ["first", "second"] {
            let path = root.join(folder);
            assert!(
                fs::read_to_string(path.join("doc.html"))?.contains("src=\"images/clock.svg\"")
            );
            assert!(path.join("images/clock.svg").is_file());
        }
        let file = convert(root, &["first/doc.adoc", "-o", "output/custom.html"], None)?;
        assert!(file.status.success(), "{}", output_text(&file.stderr));
        assert!(root.join("output/images/clock.svg").is_file());

        let stream = convert(root, &["--stdin", "-o", "-"], Some(SOURCE))?;
        assert!(stream.status.success(), "{}", output_text(&stream.stderr));
        assert!(output_text(&stream.stdout).contains("src=\"images/clock.svg\""));
        assert!(root.join("images/clock.svg").is_file());
        Ok(())
    }

    #[test]
    fn diagram_embedded_html_resolves_from_the_source_directory() -> TestResult {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        setup(root)?;
        fs::create_dir(root.join("source"))?;
        fs::write(root.join("source/doc.adoc"), SOURCE)?;
        let file = convert(
            root,
            &["source/doc.adoc", "-a", "data-uri", "-o", "doc.html"],
            None,
        )?;
        assert!(file.status.success(), "{}", output_text(&file.stderr));
        assert!(
            fs::read_to_string(root.join("doc.html"))?.contains("data:image/svg+xml;base64,PHN2Zy")
        );
        assert!(root.join("source/images/clock.svg").is_file());
        Ok(())
    }

    #[test]
    fn imagesoutdir_stages_diagrams_and_preserves_published_links() -> TestResult {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("build #1%");
        fs::create_dir(&root)?;
        setup(&root)?;
        for folder in ["source", "out"] {
            fs::create_dir(root.join(folder))?;
        }
        let source = "= Guide\n:imagesdir: public\n:imagesoutdir: staging\n\n[wavedrom,boards/clock,svg]\n....\nfirst\n....\n\n:imagesoutdir!:\n\n[wavedrom,other,svg]\n....\nsecond\n....\n";
        fs::write(root.join("source/doc.adoc"), source)?;
        let args = ["source/doc.adoc", "-o", "out/doc.html"];
        let linked = convert(&root, &args, None)?;
        assert!(linked.status.success(), "{}", output_text(&linked.stderr));
        let html = fs::read_to_string(root.join("out/doc.html"))?;
        assert!(html.contains("src=\"public/boards/clock.svg\""));
        assert!(html.contains("src=\"public/other.svg\""));
        assert!(root.join("staging/boards/clock.svg").is_file());
        assert!(root.join("out/public/other.svg").is_file());
        assert!(!root.join("out/public/boards").exists());

        let mut embedded_args = args.to_vec();
        embedded_args.extend(["-a", "data-uri"]);
        let embedded = convert(&root, &embedded_args, None)?;
        assert!(
            embedded.status.success(),
            "{}",
            output_text(&embedded.stderr)
        );
        let html = fs::read_to_string(root.join("out/doc.html"))?;
        assert_eq!(html.matches("src=\"data:image/svg+xml;base64,").count(), 2);
        assert!(root.join("source/public/other.svg").is_file());
        assert!(!root.join("source/public/boards").exists());

        fs::write(
            root.join("source/doc.adoc"),
            source.replace("boards/clock,svg]", "boards/clock,svg,opts=inline]"),
        )?;
        let inline = convert(&root, &args, None)?;
        assert!(inline.status.success(), "{}", output_text(&inline.stderr));
        let html = fs::read_to_string(root.join("out/doc.html"))?;
        assert!(html.contains("<rect ") && html.contains("src=\"public/other.svg\""));
        Ok(())
    }

    #[cfg(feature = "pdf")]
    #[test]
    fn tikz_defaults_to_an_embeddable_asset_for_pdf_output() -> TestResult {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        fs::create_dir(root.join("source"))?;
        fs::write(
            root.join("source/doc.adoc"),
            "[tikz,figure]\n....\npicture\n....\n",
        )?;
        fs::write(
            root.join("engine"),
            "#!/bin/sh\nprintf '%%PDF-1.4\\n%%%%EOF\\n' > input.pdf\n",
        )?;
        fs::write(
            root.join("pdf2svg"),
            r#"#!/bin/sh
printf '%s' '<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"/></svg>' > "$2"
"#,
        )?;
        for tool in ["engine", "pdf2svg"] {
            fs::set_permissions(root.join(tool), fs::Permissions::from_mode(0o700))?;
        }
        let args = [
            "convert",
            "--backend",
            "pdf",
            "--extension",
            "diagram",
            "--lualatex",
            "./engine",
            "--fail-on-warnings",
            "-o",
            "report.pdf",
            "source/doc.adoc",
        ];
        let missing = run_acdc_in(root, &args, None)?;
        assert!(!missing.status.success());
        assert!(output_text(&missing.stderr).contains("requires a configured pdf2svg"));
        let mut configured = args.to_vec();
        configured.extend(["--pdf2svg", "./pdf2svg"]);
        let file = run_acdc_in(root, &configured, None)?;
        assert!(file.status.success(), "{}", output_text(&file.stderr));
        assert!(root.join("source/figure.svg").is_file());
        assert!(fs::read(root.join("report.pdf"))?.starts_with(b"%PDF-"));
        configured.extend(["-a", "imagesdir=public", "-a", "imagesoutdir=staged files"]);
        let staged = run_acdc_in(root, &configured, None)?;
        assert!(staged.status.success(), "{}", output_text(&staged.stderr));
        assert!(root.join("staged files/figure.svg").is_file());
        assert!(!root.join("source/public").exists());
        Ok(())
    }

    #[test]
    fn diagram_blocks_reject_every_restricted_safe_mode_before_launch() -> TestResult {
        let dir = tempfile::tempdir()?;
        setup(dir.path())?;
        for mode in ["safe", "server", "secure"] {
            let output = convert(dir.path(), &["--safe-mode", mode, "--stdin"], Some(SOURCE))?;
            assert!(!output.status.success());
            assert!(
                output_text(&output.stderr).contains("requires unsafe mode"),
                "{}",
                output_text(&output.stderr)
            );
            assert!(!dir.path().join("images").exists());
        }
        Ok(())
    }
}
