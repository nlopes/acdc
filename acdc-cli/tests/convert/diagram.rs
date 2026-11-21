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
        (vec!["--mscgen", "missing"], "require --extension diagram"),
        (
            vec!["--graphviz-dot", "missing"],
            "require --extension diagram",
        ),
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
    fn msc_renders_block_and_local_file_with_the_selected_font() -> TestResult {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        fs::create_dir(root.join("charts"))?;
        fs::write(root.join("charts/exchange.msc"), "file chart")?;
        fs::write(
            root.join("mscgen"),
            r#"#!/bin/sh
set -eu
test "$#" = 7
test "$1" = -o && test "$3" = -T && test "$4" = svg && test "$5" = -F && test "$7" = -
case "$(cat)" in
  'file chart') test -f exchange.msc && test "$6" = 'File Font' ;;
  'block chart') test -f doc.adoc && test "$6" = 'Block Font' ;;
  *) exit 2 ;;
esac
printf '%s' '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="8"/>' > "$2"
"#,
        )?;
        fs::set_permissions(root.join("mscgen"), fs::Permissions::from_mode(0o700))?;
        fs::write(
            root.join("doc.adoc"),
            "= Charts\n:msc-font: Block Font\n:msc-format: svg\n:imagesdir: images\n\n[msc,request]\n....\nblock chart\n....\n\n.Reply\nmsc::charts/exchange.msc[svg,font=File Font,alt=Reply]",
        )?;
        let args = [
            "convert",
            "--extension",
            "diagram",
            "--mscgen",
            "./mscgen",
            "--fail-on-warnings",
            "doc.adoc",
        ];
        let run = run_acdc_in(root, &args, None)?;
        assert!(run.status.success(), "{}", output_text(&run.stderr));
        let html = fs::read_to_string(root.join("doc.html"))?;
        for (name, alt) in [("request", "request"), ("exchange", "Reply")] {
            assert!(root.join(format!("images/{name}.svg")).is_file());
            assert!(html.contains(&format!(
                r#"src="images/{name}.svg" alt="{alt}" width="16" height="8""#
            )));
        }
        assert!(html.contains("Figure 1. Reply"));
        for (source, expected) in [
            ("msc::charts/exchange.msc[pdf]", "unsupported msc format"),
            (
                ":mscgen: other\n\n[msc]\n----\nsource\n----",
                "set the mscgen executable",
            ),
            (
                "msc::charts/exchange.msc[mscgen_js=other]",
                "set the mscgen executable",
            ),
        ] {
            fs::write(root.join("doc.adoc"), source)?;
            let output = run_acdc_in(root, &args, None)?;
            assert!(!output.status.success());
            assert!(output_text(&output.stderr).contains(expected));
        }
        Ok(())
    }

    #[test]
    fn graphviz_file_macro_preserves_image_semantics_and_reads_relative_to_the_invocation()
    -> TestResult {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        fs::create_dir_all(root.join("source/parts/graphs"))?;
        fs::create_dir(root.join("out"))?;
        fs::write(
            root.join("source/parts/graphs/chart.dot"),
            "digraph { input -> output }",
        )?;
        fs::write(
            root.join("source/parts/part.adoc"),
            ".Flow\ngraphviz::graphs/chart.dot[svg,alt=\"*Graph*\"]",
        )?;
        fs::write(
            root.join("source/doc.adoc"),
            "= Document\n\n:imagesdir: images\n\ninclude::parts/part.adoc[]\n\ngraphviz::parts/graphs/chart.dot[svg]\n\n.Next\nimage::next.png[]",
        )?;
        fs::write(
            root.join("dot"),
            r#"#!/bin/sh
set -eu
test -f chart.dot
test "$(cat)" = 'digraph { input -> output }'
test "$1" = -o
test "$3" = -Tsvg
printf '%s' '<svg xmlns="http://www.w3.org/2000/svg" width="10" height="20"/>' > "$2"
"#,
        )?;
        fs::set_permissions(root.join("dot"), fs::Permissions::from_mode(0o700))?;
        let args = [
            "convert",
            "--extension",
            "diagram",
            "--graphviz-dot",
            "./dot",
            "--fail-on-warnings",
            "source/doc.adoc",
            "-o",
            "out/doc.html",
        ];
        let run = run_acdc_in(root, &args, None)?;
        assert!(run.status.success(), "{}", output_text(&run.stderr));
        let html = fs::read_to_string(root.join("out/doc.html"))?;
        assert!(
            html.contains(r#"src="images/chart.svg" alt="*Graph*" width="10" height="20""#),
            "{html}"
        );
        assert!(html.contains(r#"src="images/chart.svg" alt="Diagram""#));
        assert!(html.contains("Figure 1. Flow") && html.contains("Figure 2. Next"));
        assert!(root.join("out/images/chart.svg").is_file());

        fs::write(root.join("source/doc.adoc"), "graphviz::missing.dot[svg]")?;
        let mut safe_args = args.to_vec();
        safe_args.extend(["--safe-mode", "secure"]);
        let denied = run_acdc_in(root, &safe_args, None)?;
        assert!(!denied.status.success());
        assert!(output_text(&denied.stderr).contains("Graphviz requires unsafe mode"));
        let run = run_acdc_in(root, &args, None)?;
        assert!(!run.status.success());
        let html = fs::read_to_string(root.join("out/doc.html"))?;
        assert!(
            html.contains("Failed to generate image")
                && html.contains("graphviz::missing.dot[svg]")
        );
        Ok(())
    }

    #[test]
    fn graphviz_file_macro_preserves_its_input_on_an_output_collision() -> TestResult {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        setup(root)?;
        fs::write(root.join("chart.svg"), "digraph { keep }")?;
        fs::write(root.join("doc.adoc"), "graphviz::chart.svg[svg]")?;
        let result = run_acdc_in(
            root,
            &[
                "convert",
                "--extension",
                "diagram",
                "--graphviz-dot",
                "./renderer",
                "--fail-on-warnings",
                "doc.adoc",
            ],
            None,
        )?;
        assert!(!result.status.success());
        assert!(output_text(&result.stderr).contains("overwrite its source file"));
        assert_eq!(
            fs::read_to_string(root.join("chart.svg"))?,
            "digraph { keep }"
        );
        Ok(())
    }

    #[test]
    fn graphviz_uses_the_configured_dot_and_source_position_layout() -> TestResult {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        fs::create_dir(root.join("source"))?;
        fs::create_dir(root.join("out"))?;
        fs::write(
            root.join("dot"),
            r#"#!/bin/sh
set -eu
test -f graph.adoc
test "$1" = -o
test "$3" = -Tsvg
test "$(cat)" = 'digraph { start -> finish }'
printf '%s\n' "$4" >> layouts
cat > "$2" <<'SVG'
<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN"
 "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
<svg xmlns="http://www.w3.org/2000/svg" width="80pt" height="20pt"/>
SVG
"#,
        )?;
        fs::set_permissions(root.join("dot"), fs::Permissions::from_mode(0o700))?;
        let body = "digraph { start -> finish }";
        fs::write(
            root.join("source/graph.adoc"),
            format!(
                "= Flow\n:graphviz-format: svg\n:graphviz-layout: dot\n\n[graphviz]\n----\n{body}\n----\n\n[graphviz,layout=neato]\n....\n{body}\n...."
            ),
        )?;
        let args = [
            "convert",
            "--extension",
            "diagram",
            "--graphviz-dot",
            "./dot",
            "--fail-on-warnings",
            "source/graph.adoc",
            "-o",
            "out/graph.html",
        ];
        let output = run_acdc_in(root, &args, None)?;
        assert!(output.status.success(), "{}", output_text(&output.stderr));
        assert_eq!(
            fs::read_to_string(root.join("source/layouts"))?,
            "-Kdot\n-Kneato\n"
        );
        let html = fs::read_to_string(root.join("out/graph.html"))?;
        let mut diagrams = 0;
        for entry in fs::read_dir(root.join("out"))? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "svg") {
                let name = entry.file_name();
                assert!(html.contains(&format!("src=\"{}\"", name.to_string_lossy())));
                diagrams += 1;
            }
        }
        assert_eq!(diagrams, 2);
        assert_eq!(html.matches(r#"width="106" height="26""#).count(), 2);

        fs::write(
            root.join("source/graph.adoc"),
            format!("[graphviz,graphvizdot=other]\n....\n{body}\n...."),
        )?;
        let denied = run_acdc_in(root, &args, None)?;
        assert!(!denied.status.success());
        assert!(output_text(&denied.stderr).contains("set the Graphviz executable"));
        assert_eq!(
            fs::read_to_string(root.join("source/layouts"))?,
            "-Kdot\n-Kneato\n"
        );
        Ok(())
    }

    #[test]
    fn renderer_scratch_is_private_with_a_permissive_umask() -> TestResult {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        setup(root)?;
        fs::write(root.join("source.adoc"), SOURCE)?;
        fs::write(
            root.join("renderer"),
            r#"#!/bin/sh
set -eu
pwd > ../scratch-path
ls -ld . > ../scratch-mode
printf '%s' '<svg xmlns="http://www.w3.org/2000/svg"/>' > "$4"
"#,
        )?;
        let output = std::process::Command::new("sh")
            .args([
                "-c",
                r#"umask 000; exec "$@""#,
                "sh",
                env!("CARGO_BIN_EXE_acdc"),
                "convert",
                "--extension",
                "diagram",
                "--wavedrom-cli",
                "./renderer",
                "--fail-on-warnings",
                "source.adoc",
            ])
            .current_dir(root)
            .output()?;
        assert!(output.status.success(), "{}", output_text(&output.stderr));
        let mode = fs::read_to_string(root.join("images/scratch-mode"))?;
        // The parent filesystem may propagate setgid without granting group access.
        assert!(
            mode.starts_with("drwx------") || mode.starts_with("drwx--S---"),
            "{mode}"
        );
        let scratch = fs::read_to_string(root.join("images/scratch-path"))?;
        assert!(!Path::new(scratch.trim_end()).exists());
        assert!(root.join("images/clock.svg").is_file());
        Ok(())
    }

    #[test]
    fn svg_type_follows_block_options_and_scoped_document_defaults() -> TestResult {
        use std::fmt::Write as _;

        let blocks = [
            ("", "shared,svg,opts=other"),
            (":wavedrom-svg-type: interactive", "renderer,svg"),
            ("", "local,svg,svg-type=static"),
            ("", "explicit,svg,svg-type=unknown,opts=inline"),
            (":wavedrom-svg-type!:", "inherited,svg"),
            ("", "object,svg,opts=interactive"),
            ("", r#"both,svg,opts="interactive,inline""#),
            (":diagram-svg-type!:", "plain,svg"),
        ];
        let mut doc = String::from("= Guide\n:diagram-svg-type: inline\n");
        for (attrs, block) in blocks {
            writeln!(doc, "\n{attrs}\n\n[wavedrom,{block}]\n....\nclock\n....")?;
        }
        for mode in ["asciidoctor", "draft"] {
            let dir = tempfile::tempdir()?;
            let root = dir.path();
            setup(root)?;
            fs::create_dir(root.join("source"))?;
            fs::create_dir(root.join("out"))?;
            fs::write(root.join("source/doc.adoc"), &doc)?;
            let result = convert(
                root,
                &[
                    "--embedded",
                    "--compatibility-mode",
                    mode,
                    "source/doc.adoc",
                    "-o",
                    "out/doc.html",
                ],
                None,
            )?;
            assert!(result.status.success(), "{}", output_text(&result.stderr));
            let html = fs::read_to_string(root.join("out/doc.html"))?;
            assert_eq!(html.matches("<svg ").count(), 4, "{html}");
            for name in ["shared", "explicit", "inherited", "both"] {
                assert!(root.join(format!("source/{name}.svg")).is_file());
            }
            for name in ["renderer", "object"] {
                assert!(
                    html.contains(&format!(
                        r#"<object type="image/svg+xml" data="{name}.svg""#
                    )),
                    "{html}"
                );
                assert!(root.join(format!("out/{name}.svg")).is_file());
            }
            for name in ["local", "plain"] {
                assert!(
                    html.contains(&format!(r#"<img src="{name}.svg""#)),
                    "{html}"
                );
                assert!(root.join(format!("out/{name}.svg")).is_file());
            }
        }
        Ok(())
    }

    #[test]
    fn svg_dimensions_supply_independent_html_defaults() -> TestResult {
        let dir = tempfile::tempdir()?;
        setup(dir.path())?;
        fs::write(dir.path().join("renderer"), "#!/bin/sh\ncp \"$2\" \"$4\"\n")?;
        let pixels = r#"width="240px" height="30" viewBox="0 0 1 1""#;
        let cases = [
            (",svg", pixels, r#"alt="Diagram" width="240" height="30">"#),
            (
                "clock,svg,width=50%,scale=0.5",
                pixels,
                r#"alt="clock" width="50%" height="15">"#,
            ),
            (
                "reset,svg,height=7",
                pixels,
                r#"alt="reset" width="240" height="7">"#,
            ),
            (
                "both,svg,width=12,height=23",
                pixels,
                r#"alt="both" width="12" height="23">"#,
            ),
            (
                "points,svg",
                r#"width="383.212pt" height="250.908pt""#,
                r#"alt="points" width="509" height="333">"#,
            ),
            (
                "box,svg",
                r#"width="100%" viewBox="4 6 80 24""#,
                r#"alt="box" width="80" height="24">"#,
            ),
            ("unknown,svg", r#"width="100%""#, r#"alt="unknown">"#),
        ];
        let source = cases.map(|(attributes, geometry, _)| {
            format!("[wavedrom,{attributes}]\n....\n<svg xmlns=\"http://www.w3.org/2000/svg\" {geometry}/>\n....")
        }).join("\n\n");
        let output = convert(dir.path(), &["--stdin", "--embedded"], Some(&source))?;
        assert!(output.status.success(), "{}", output_text(&output.stderr));
        let html = output_text(&output.stdout);
        for (_, _, expected) in cases {
            assert!(html.contains(expected), "missing {expected} in {html}");
        }
        Ok(())
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
    fn nested_diagrams_preserve_references_captions_and_cell_local_output() -> TestResult {
        let source = include_str!("fixtures/nested-diagrams.adoc");
        for mode in ["asciidoctor", "draft"] {
            let dir = tempfile::tempdir()?;
            setup(dir.path())?;
            let output = convert(
                dir.path(),
                &["--stdin", "--embedded", "--compatibility-mode", mode],
                Some(source),
            )?;
            assert!(
                output.status.success(),
                "{mode}: {}",
                output_text(&output.stderr)
            );

            let html = output_text(&output.stdout);
            let mut remaining = html.as_str();
            for fragment in [
                r##"href="#clock">Clock trace</a>"##,
                r#"class="sidebarblock""#,
                "<li>",
                r#"id="clock" class="imageblock timing""#,
                r#"src="public/clock.svg""#,
                "Figure 1. Clock trace",
                "</li>",
                "<table ",
                "<td ",
                r#"src="public/reset.svg""#,
                "</td>",
                "<td ",
                r#"src="public/enable.svg""#,
                "</table>",
                r#"src="public/after.svg""#,
                "Figure 2. Output trace",
            ] {
                (_, remaining) = remaining
                    .split_once(fragment)
                    .ok_or_else(|| format!("{mode}: missing or misplaced {fragment}"))?;
            }
            for asset in [
                "public/clock.svg",
                "staging/reset.svg",
                "public/enable.svg",
                "public/after.svg",
            ] {
                assert!(dir.path().join(asset).is_file(), "{mode}: missing {asset}");
            }
            assert!(!dir.path().join("public/reset.svg").exists());
        }
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
