use acdc_parser::{
    CompatibilityMode, Error, Options, SafeMode, ast::BlockKind, extensions::Registry,
};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

const ACE: &str = include_str!("fixtures/ace-state.adoc");
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn registry(
    engine: &Path,
    converter: Option<&Path>,
    root: &Path,
) -> Result<Registry, Box<dyn std::error::Error>> {
    let mut builder = Registry::builder();
    builder.register_block_style(acdc_diagram::tikz(engine, converter, root)?)?;
    Ok(builder.build())
}

#[test]
fn tikz_rejects_document_tool_selection_and_requires_unsafe_mode() -> TestResult {
    let dir = tempfile::tempdir()?;
    let dummy = dir.path().join("not-an-executable");
    fs::write(&dummy, "")?;
    let registry = registry(&dummy, None, dir.path())?;
    for mode in [SafeMode::Secure, SafeMode::Server, SafeMode::Safe] {
        let result = registry.parse(ACE, &Options::builder().with_safe_mode(mode).build()?);
        assert!(
            matches!(result, Err(Error::Extension(origin, msg)) if msg.contains("unsafe mode") && origin.location.start.line == 1)
        );
    }
    let unsafe_opts = Options::builder()
        .with_attribute("diagram-on-error", "abort")
        .with_safe_mode(SafeMode::Unsafe)
        .build()?;
    for (prefix, message) in [
        ("[tikz,command=/other/tool]", "command=lualatex"),
        (":tikz-command: xelatex\n\n[tikz]", "command=lualatex"),
        (":pdf2svg: untrusted\n\n[tikz]", "registering"),
        ("[tikz,,svg]", "configured pdf2svg"),
        ("[tikz,,png]", "unsupported TikZ format"),
    ] {
        let result = registry.parse(&format!("{prefix}\n....\nbody\n...."), &unsafe_opts);
        assert!(matches!(result, Err(Error::Extension(_, msg)) if msg.contains(message)));
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn latex_runs_with_fixed_arguments_in_scratch_and_publishes_only_on_success() -> TestResult {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir()?;
    let engine = dir.path().join("configured engine");
    fs::write(
        &engine,
        r#"#!/bin/sh
set -eu
test "$*" = "-no-shell-escape -halt-on-error -file-line-error -interaction=nonstopmode input.tex"
cmp input.tex "$(dirname "$0")/expected.tex"
printf '%%PDF-1.4\n%%%%EOF\n' > input.pdf
"#,
    )?;
    fs::set_permissions(&engine, fs::Permissions::from_mode(0o700))?;
    fs::write(
        dir.path().join("expected.tex"),
        concat!(
            "\\documentclass[border=2bp, tikz]{standalone}\n\\usepackage{tikz}\n",
            "% preamble\n\\begin{document}\n\\begingroup\n\\tikzset{every picture/.style={scale=1}}\n",
            "picture body\n\n\\endgroup\n\\end{document}\n"
        ),
    )?;
    let registry = registry(&engine, None, dir.path())?;
    let options = Options::builder()
        .with_attribute("diagram-on-error", "abort")
        .with_safe_mode(SafeMode::Unsafe)
        .build()?;
    let source = "[tikz,figure,pdf,preamble=true]\n....\n% preamble\n~~~~\npicture body\n....";
    let listing = source.replace("....", "----");
    let substituted = format!(
        ":header-text: preamble\n:node-text: picture body\n\n{}",
        source
            .replace("preamble=true]", "preamble=true,subs=attributes]")
            .replace("% preamble", "% {header-text}")
            .replace("picture body", "{node-text}")
    );
    for input in [source, listing.as_str(), substituted.as_str()] {
        let parsed = registry.parse(input, &options)?.finish()?;
        assert!(parsed.document().blocks().any(
            |b| matches!(b.kind(), BlockKind::Image(i) if i.source().to_string()=="figure.pdf")
        ));
    }
    let result = registry.parse(&source.replace("picture body", "different"), &options);
    assert!(matches!(result, Err(Error::Extension(_, msg)) if msg.contains("lualatex exited")));
    assert_eq!(
        fs::read(dir.path().join("figure.pdf"))?,
        b"%PDF-1.4\n%%EOF\n"
    );
    fs::write(&engine, "#!/bin/sh\nprintf '%%PDF-1.4\\n' > input.pdf\n")?;
    let result = registry.parse(source, &options);
    assert!(matches!(result, Err(Error::Extension(_, msg)) if msg.contains("invalid PDF")));
    assert_eq!(
        fs::read(dir.path().join("figure.pdf"))?,
        b"%PDF-1.4\n%%EOF\n"
    );
    Ok(())
}

#[test]
#[ignore = "requires LuaLaTeX, the ACE Petrona fonts, and pdf2svg"]
fn ace_state_diagram_with_real_lualatex() -> TestResult {
    let engine = PathBuf::from(env::var_os("ACDC_LUALATEX").ok_or("set ACDC_LUALATEX")?);
    let converter = PathBuf::from(env::var_os("ACDC_PDF2SVG").ok_or("set ACDC_PDF2SVG")?);
    for mode in [CompatibilityMode::Draft, CompatibilityMode::Asciidoctor] {
        let dir = tempfile::tempdir()?;
        let registry = registry(&engine, Some(&converter), dir.path())?;
        let options = Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?;
        for format in ["svg", "pdf"] {
            let source = format!(
                ":imagesdir: diagrams\n\n{}",
                ACE.replacen("tikz,,svg", &format!("tikz,,{format}"), 1)
            );
            let parsed = registry.parse(&source, &options)?.finish()?;
            assert_eq!(parsed.warnings(), []);
            let image = parsed
                .document()
                .blocks()
                .find_map(|b| {
                    if let BlockKind::Image(i) = b.kind() {
                        Some(i)
                    } else {
                        None
                    }
                })
                .ok_or("expected ordinary image")?;
            let target = image.source().to_string();
            assert!(target.ends_with(&format!(".{format}")));
            let asset = dir.path().join("diagrams").join(target);
            if format == "svg" {
                let svg = fs::read_to_string(asset)?;
                let doc = roxmltree::Document::parse(&svg)?;
                assert!(
                    doc.root_element()
                        .has_tag_name(("http://www.w3.org/2000/svg", "svg"))
                );
                assert!(doc.descendants().any(|n| n.has_tag_name("path")));
            } else {
                let pdf = lopdf::Document::load(asset)?;
                assert_eq!(pdf.get_pages().len(), 1);
                let text = pdf.extract_text(&[1])?;
                assert!(
                    text.contains("Initial") && text.contains("Expiration"),
                    "{text}"
                );
            }
        }
    }
    Ok(())
}
