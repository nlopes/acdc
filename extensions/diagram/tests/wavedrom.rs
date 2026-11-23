use std::{
    env, fs,
    io::BufReader,
    path::{Path, PathBuf},
};

use acdc_converters_core::{
    Converter as _, Diagnostics, Options as ConverterOptions, pipeline::Pipeline,
};
use acdc_converters_html::{Converter, HtmlVariant};
use acdc_parser::{
    CompatibilityMode, Error, Options, SafeMode, ast::BlockKind, extensions::Registry,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const ACE: &str = include_str!("fixtures/ace-load.adoc");

fn registry(tool: &Path, root: &Path) -> Result<Registry, Box<dyn std::error::Error>> {
    let mut builder = Registry::builder();
    builder.register_block_style(acdc_diagram::wavedrom(tool, root)?)?;
    Ok(builder.build())
}

fn render_asset(
    tool: &Path,
    root: &Path,
    mode: CompatibilityMode,
    source_text: &str,
    expected_html_alt: &str,
) -> Result<(PathBuf, String), Box<dyn std::error::Error>> {
    let pipeline = Pipeline::new(Converter::new_with_variant(
        ConverterOptions::builder().embedded(true).build(),
        Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe),
        HtmlVariant::Standard,
    )?)
    .with_extensions(registry(tool, root)?);
    let parsed = pipeline.parse(&format!("Intro.\n\n:imagesdir: images\n\n{source_text}"))?;
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    let target = parsed
        .document()
        .blocks()
        .find_map(|block| {
            if let BlockKind::Image(image) = block.kind() {
                Some(image.source().to_string())
            } else {
                None
            }
        })
        .ok_or("expected an ordinary image block")?;
    let mut html = Vec::new();
    let mut warnings = Vec::new();
    let source = pipeline.converter().warning_source();
    pipeline.write_to(
        parsed.document(),
        &mut html,
        None,
        None,
        &mut Diagnostics::new(&source, &mut warnings),
    )?;
    let reference = format!("images/{target}");
    let html = String::from_utf8(html)?;
    assert!(html.contains(&format!("src=\"{reference}\"")));
    assert!(
        html.contains(&format!("alt=\"{expected_html_alt}\"")),
        "{html}"
    );
    assert_eq!(warnings, []);
    Ok((root.join(reference), html))
}

#[test]
#[ignore = "requires the real wavedrom-cli; set ACDC_WAVEDROM_CLI to its path"]
fn ace_register_diagram_with_real_wavedrom_cli() -> TestResult {
    let tool = PathBuf::from(env::var_os("ACDC_WAVEDROM_CLI").ok_or("set ACDC_WAVEDROM_CLI")?);
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let output = tempfile::tempdir()?;
        let (path, _) = render_asset(&tool, output.path(), mode, ACE, "Diagram")?;
        let svg = fs::read_to_string(path)?;
        let document = roxmltree::Document::parse(&svg)?;
        let root = document.root_element();
        assert!(root.has_tag_name(("http://www.w3.org/2000/svg", "svg")));
        assert_eq!(root.attribute("viewBox"), Some("0 0 800 84"));
        for label in ["ace.load", "context", "immed12", "K{Xd}"] {
            assert!(
                root.descendants()
                    .any(|node| node.is_text() && node.text() == Some(label)),
                "missing {label}"
            );
        }

        let png_source = ACE.replacen(", svg]", ", png]", 1);
        let (path, html) = render_asset(&tool, output.path(), mode, &png_source, "Diagram")?;
        assert!(html.contains(r#"width="800" height="84""#), "{html}");
        assert_eq!(path.extension().and_then(|ext| ext.to_str()), Some("png"));
        let mut reader = png::Decoder::new(BufReader::new(fs::File::open(path)?)).read_info()?;
        assert_eq!(reader.info().size(), (800, 84));
        let mut pixels = vec![0; reader.output_buffer_size().ok_or("no PNG raster")?];
        let frame = reader.next_frame(&mut pixels)?;
        assert_eq!((frame.width, frame.height), (800, 84));
    }
    Ok(())
}

#[test]
fn restrictive_modes_and_invalid_targets_fail_before_execution() -> TestResult {
    let output = tempfile::tempdir()?;
    let dummy = output.path().join("not-an-executable");
    fs::write(&dummy, "")?;
    let registry = registry(&dummy, output.path())?;
    for mode in [SafeMode::Safe, SafeMode::Server, SafeMode::Secure] {
        let options = Options::builder().with_safe_mode(mode).build()?;
        let result = registry.parse(&format!(":safe-mode-name: unsafe\n\n{ACE}"), &options);
        assert!(
            matches!(result, Err(Error::Extension(_, msg)) if msg.contains("requires unsafe mode"))
        );
    }
    let options = Options::builder()
        .with_attribute("diagram-on-error", "abort")
        .with_safe_mode(SafeMode::Unsafe)
        .build()?;
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir()?;
        std::os::unix::fs::symlink(outside.path(), output.path().join("linked"))?;
        let result = registry.parse(&format!(":imagesdir: linked\n\n{ACE}"), &options);
        assert!(matches!(result, Err(Error::Extension(_, msg)) if msg.contains("imagesdir")));
        let result = registry.parse(&format!(":imagesoutdir: linked\n\n{ACE}"), &options);
        assert!(
            matches!(result, Err(Error::Extension(_, msg)) if msg.contains("imagesoutdir") && msg.contains("real directories"))
        );
        let result = registry.parse("[wavedrom,linked/chart,svg]\n....\nbody\n....", &options);
        assert!(
            matches!(result, Err(Error::Extension(_, msg)) if msg.contains("target") && msg.contains("real directories"))
        );
        assert_eq!(fs::read_dir(outside.path())?.count(), 0);
    }
    for (header, fragment) in [
        ("[wavedrom,../outside,svg]", "target"),
        ("[wavedrom,boards/../clock,svg]", "target"),
        ("[wavedrom,/boards/clock,svg]", "target"),
        ("[wavedrom,boards//clock,svg]", "target"),
        ("[wavedrom,boards/,svg]", "target"),
        ("[wavedrom,,pdf]", "format"),
        (
            ":diagram-svg-type: unknown\n\n[wavedrom]",
            "unsupported SVG type",
        ),
        ("[wavedrom,chart,svg,svg-type=]", "unsupported SVG type"),
        (":wavedrom: other-tool\n\n[wavedrom]", "executable"),
        (":imagesoutdir: ../outside\n\n[wavedrom]", "imagesoutdir"),
        (":imagesoutdir: /outside\n\n[wavedrom]", "imagesoutdir"),
        (":imagesdir: ../outside\n\n[wavedrom]", "imagesdir"),
    ] {
        let result = registry.parse(&format!("{header}\n....\nbody\n...."), &options);
        assert!(matches!(result, Err(Error::Extension(_, msg)) if msg.contains(fragment)));
    }
    Ok(())
}

#[test]
fn error_policy_uses_block_renderer_and_document_attributes_in_source_order() -> TestResult {
    let root = tempfile::tempdir()?;
    let tool = root.path().join("unused");
    fs::write(&tool, "")?;
    let registry = registry(&tool, root.path())?;
    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .build()?;
    let block = "[wavedrom,,pdf]\n....\nbody\n....";
    let source = format!(
        "{block}\n\n:diagram-on-error: abort\n:wavedrom-on-error: log\n\n{block}\n\n:wavedrom-on-error: abort\n\n[wavedrom,,pdf,on-error=log]\n....\nbody\n...."
    );
    let parsed = registry.parse(&source, &options)?.finish()?;
    assert_eq!(parsed.warnings().len(), 3);
    assert_eq!(
        parsed
            .document()
            .blocks()
            .filter(|b| matches!(b.kind(), BlockKind::Listing(_)))
            .count(),
        3
    );

    for prefix in [":wavedrom-on-error!:", ":diagram-on-error: log"] {
        let source = format!("{source}\n\n{prefix}\n\n{block}");
        let last_block_line = u32::try_from(source.lines().count() - 3)?;
        assert!(
            matches!(registry.parse(&source, &options), Err(Error::Extension(origin, message))
            if origin.location.start.line == last_block_line && message.contains("unsupported WaveDrom format"))
        );
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn exact_raw_body_is_passed_to_the_renderer_and_its_svg_is_published() -> TestResult {
    use std::os::unix::fs::PermissionsExt as _;

    let root = tempfile::tempdir()?;
    let tool = root.path().join("renderer");
    fs::write(
        &tool,
        r#"#!/bin/sh
set -eu
test "$1" = --input
test "$3" = --svg
cmp "$2" "$(dirname "$0")/expected.wvd"
printf '%s' '<svg xmlns="http://www.w3.org/2000/svg"><desc>test</desc></svg>' > "$4"
"#,
    )?;
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o700))?;
    let (_, delimited) = ACE.split_once("....\n").ok_or("opening delimiter")?;
    fs::write(
        root.path().join("expected.wvd"),
        delimited
            .strip_suffix("....\n")
            .ok_or("closing delimiter")?,
    )?;
    let listing = ACE.replace("\n....\n", "\n----\n");
    let nested = ACE.replacen(", ,", ",boards/rev-a/clock_signal,", 1);
    let (path, _) = render_asset(
        &tool,
        root.path(),
        CompatibilityMode::Draft,
        &nested,
        "clock signal",
    )?;
    assert_eq!(
        path,
        root.path().join("images/boards/rev-a/clock_signal.svg")
    );
    assert!(path.is_file());
    let named = ACE.replacen(", ,", ",clock-signal_v2,", 1);
    let labeled = named.replacen("svg]", "svg,alt=\"Clock & reset\"]", 1);
    let decorative = ACE.replacen("svg]", "svg,alt=\"\"]", 1);
    for (source, alt) in [
        (ACE, "Diagram"),
        (listing.as_str(), "Diagram"),
        (&named, "clock signal v2"),
        (&labeled, "Clock &amp; reset"),
        (&decorative, ""),
    ] {
        let (path, _) = render_asset(&tool, root.path(), CompatibilityMode::Draft, source, alt)?;
        let svg = fs::read_to_string(path)?;
        assert!(svg.contains("<desc>test</desc>"));
    }

    // A failed rerender must preserve an existing named asset.
    let registry = registry(&tool, root.path())?;
    let options = Options::builder()
        .with_attribute("diagram-on-error", "abort")
        .with_safe_mode(SafeMode::Unsafe)
        .build()?;
    fs::write(root.path().join("existing.svg"), "old asset")?;
    let result = registry.parse("[wavedrom,existing,svg]\n....\nbad input\n....", &options);
    assert!(
        matches!(result, Err(Error::Extension(origin, msg)) if origin.location.start.line == 1 && msg.contains("renderer exited"))
    );
    assert_eq!(
        fs::read_to_string(root.path().join("existing.svg"))?,
        "old asset"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn attribute_substitutions_use_local_values_for_renderer_input_and_asset_names() -> TestResult {
    use std::{collections::HashSet, fmt::Write as _, os::unix::fs::PermissionsExt as _};

    let dir = tempfile::tempdir()?;
    let tool = dir.path().join("renderer");
    fs::write(
        &tool,
        r#"#!/bin/sh
set -eu
{
    printf '%s' '<svg xmlns="http://www.w3.org/2000/svg"><desc><![CDATA['
    cat "$2"
    printf '%s' ']]></desc></svg>'
} > "$4"
"#,
    )?;
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o700))?;
    let registry = registry(&tool, dir.path())?;
    let raw = r"{label} \{label} {unknown} *raw* <1>";
    let first = "Early {label} {unknown} *raw* <1>";
    let second = "Late {label} {unknown} *raw* <1>";
    let unset = "{label} {label} {unknown} *raw* <1>";
    let mut source = String::from("= Signals\n:label: Early\n\n");
    for (attribute, style) in [
        ("", "wavedrom,subs=attributes"),
        (":label: Late\n\n", "wavedrom,subs=+attributes"),
        ("", "wavedrom,subs=normal"),
        ("", "wavedrom"),
        ("", "wavedrom,subs=none"),
        ("", "wavedrom,subs=-attributes"),
        (":label!:\n\n", "wavedrom,subs=attributes"),
    ] {
        writeln!(source, "{attribute}[{style}]\n----\n{raw}\n----\n")?;
    }

    for mode in [CompatibilityMode::Draft, CompatibilityMode::Asciidoctor] {
        let opts = Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?;
        let result = registry.parse(&source, &opts)?.finish()?;
        let targets: Vec<_> = result
            .document()
            .blocks()
            .filter_map(|block| {
                if let BlockKind::Image(image) = block.kind() {
                    Some(image.source().to_string())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(targets.len(), 7);
        for (name, expected) in targets
            .iter()
            .zip([first, second, second, raw, raw, raw, unset])
        {
            let svg = fs::read_to_string(dir.path().join(name))?;
            assert!(svg.contains(&format!("<![CDATA[{expected}\n]]>")), "{svg}");
        }
        assert_eq!(targets.iter().collect::<HashSet<_>>().len(), 4);
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn png_selection_and_failed_publication() -> TestResult {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir()?;
    let tool = dir.path().join("renderer");
    fs::write(
        &tool,
        r#"#!/bin/sh
set -eu
test "$1" = --input
test "$3" = --png
cp "$(dirname "$0")/result.png" "$4"
"#,
    )?;
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o700))?;

    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, 6, 4);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&[127; 6 * 4 * 3])?;
    writer.finish()?;
    let result = dir.path().join("result.png");
    fs::write(&result, &png)?;
    for header in [
        ":diagram-format: png\n\n[wavedrom,register]",
        ":diagram-format: svg\n:wavedrom-format: png\n\n[wavedrom,register]",
        ":diagram-format: png\n:wavedrom-format: svg\n\n:wavedrom-format!:\n\n[wavedrom,register]",
        ":wavedrom-format: png\n\n[wavedrom,register]",
        ":wavedrom-format: svg\n\n[wavedrom,register,png]",
        "[wavedrom,register,svg,format=png]",
        ":diagram-svg-type: unknown\n\n[wavedrom,register,png]",
    ] {
        let source = format!("{header}\n....\nbody\n....");
        let (path, html) = render_asset(
            &tool,
            dir.path(),
            CompatibilityMode::Draft,
            &source,
            "register",
        )?;
        assert_eq!(path, dir.path().join("images/register.png"));
        assert_eq!(fs::read(path)?, png);
        assert!(html.contains(r#"width="6" height="4""#), "{html}");
    }

    let (_, html) = render_asset(
        &tool,
        dir.path(),
        CompatibilityMode::Asciidoctor,
        "[wavedrom,register,png,width=50%,scale=0.5]\n....\nbody\n....",
        "register",
    )?;
    assert!(html.contains(r#"width="50%" height="2""#), "{html}");

    // A successful process can still produce a truncated PNG without an IEND chunk.
    png.truncate(png.len() - 12);
    fs::write(result, &png)?;
    let original = dir.path().join("images/register.png");
    let before = fs::read(&original)?;
    let registry = registry(&tool, dir.path())?;
    let options = Options::builder()
        .with_attribute("diagram-on-error", "abort")
        .with_safe_mode(SafeMode::Unsafe)
        .build()?;
    let failed = registry.parse(
        ":imagesdir: images\n\n[wavedrom,register,png]\n....\nbody\n....",
        &options,
    );
    assert!(
        matches!(failed, Err(Error::Extension(origin, msg)) if origin.location.start.line == 3 && msg.contains("invalid PNG"))
    );
    assert_eq!(fs::read(original)?, before);
    Ok(())
}
