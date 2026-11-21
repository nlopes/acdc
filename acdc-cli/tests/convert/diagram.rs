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
