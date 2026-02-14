use std::path::{Path, PathBuf};

use acdc_converters_core::{Backend, GeneratorMetadata, Options as ConverterOptions};
use acdc_converters_dev::output::remove_lines_trailing_whitespace;
use acdc_converters_html::{HtmlVariant, Processor, RenderOptions};
use acdc_parser::{AttributeValue, Options as ParserOptions, SafeMode};

type Error = Box<dyn std::error::Error>;

fn run_fixture_test(
    path: &Path,
    expected_dir: &Path,
    variant: HtmlVariant,
    embedded: bool,
) -> Result<(), Error> {
    let file_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Invalid fixture file name")?;
    let expected_path = expected_dir.join(file_name).with_extension("html");

    let parser_options =
        ParserOptions::with_attributes(acdc_converters_core::default_rendering_attributes());
    let doc = acdc_parser::parse_file(path, &parser_options)?;

    let backend = match variant {
        HtmlVariant::Semantic => Backend::Html5s,
        HtmlVariant::Standard => Backend::Html,
    };
    let converter_options = ConverterOptions::builder()
        .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
        .backend(backend)
        .build();
    let processor = Processor::new_with_variant(converter_options, doc.attributes.clone(), variant);
    let render_options = RenderOptions {
        embedded,
        ..RenderOptions::default()
    };

    let mut output = Vec::new();
    processor.convert_to_writer(&doc, &mut output, &render_options)?;

    let expected = std::fs::read_to_string(&expected_path)?;
    let actual = String::from_utf8(output)?;
    let expected_normalized = remove_lines_trailing_whitespace(&expected);
    let actual_normalized = remove_lines_trailing_whitespace(&actual);

    pretty_assertions::assert_eq!(
        expected_normalized,
        actual_normalized,
        "HTML output mismatch for fixture: {file_name}",
    );
    Ok(())
}

#[rstest::rstest]
#[tracing_test::traced_test]
fn test_with_fixtures(#[files("tests/fixtures/source/*.adoc")] path: PathBuf) -> Result<(), Error> {
    run_fixture_test(
        &path,
        Path::new("tests/fixtures/expected"),
        HtmlVariant::Standard,
        false,
    )
}

#[rstest::rstest]
#[tracing_test::traced_test]
fn test_html5s_with_fixtures(
    #[files("tests/fixtures/source/html5s/*.adoc")] path: PathBuf,
) -> Result<(), Error> {
    run_fixture_test(
        &path,
        Path::new("tests/fixtures/expected/html5s"),
        HtmlVariant::Semantic,
        true,
    )
}

/// Helper: convert an `AsciiDoc` string to full-page HTML with custom attributes.
fn convert_string(input: &str, extra_attrs: &[(&str, AttributeValue)]) -> Result<String, Error> {
    let mut attrs = acdc_converters_core::default_rendering_attributes();
    for (k, v) in extra_attrs {
        attrs.insert((*k).into(), v.clone());
    }
    let parser_options = ParserOptions::with_attributes(attrs);
    let doc = acdc_parser::parse(input, &parser_options)?;
    let converter_options = ConverterOptions::builder()
        .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
        .backend(Backend::Html)
        .build();
    let processor = Processor::new_with_variant(
        converter_options,
        doc.attributes.clone(),
        HtmlVariant::Standard,
    );
    let render_options = RenderOptions::default();
    let mut output = Vec::new();
    processor.convert_to_writer(&doc, &mut output, &render_options)?;
    Ok(String::from_utf8(output)?)
}

#[cfg(feature = "highlighting")]
mod syntax_highlighting {
    use super::*;

    const SOURCE_BLOCK: &str = r#":source-highlighter: syntect

[source,rust]
----
fn main() {
    println!("hello");
}
----
"#;

    #[test]
    fn class_mode_produces_class_spans() -> Result<(), Error> {
        let html = convert_string(
            SOURCE_BLOCK,
            &[("syntect-css", AttributeValue::String("class".into()))],
        )?;
        assert!(
            html.contains("class=\"syntax-"),
            "Should contain class=\"syntax-\" spans:\n{html}"
        );
        assert!(
            !html.contains("style=\"color:"),
            "Should not contain inline style= color:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn class_mode_embeds_css_in_head() -> Result<(), Error> {
        let html = convert_string(
            SOURCE_BLOCK,
            &[("syntect-css", AttributeValue::String("class".into()))],
        )?;
        assert!(
            html.contains(".syntax-"),
            "Head should contain .syntax- CSS rules:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn inline_mode_uses_style_attributes() -> Result<(), Error> {
        let html = convert_string(SOURCE_BLOCK, &[])?;
        assert!(
            html.contains("style=\""),
            "Inline mode should use style= attributes:\n{html}"
        );
        assert!(
            !html.contains("class=\"syntax-"),
            "Inline mode should not contain syntax- classes:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn syntect_style_overrides_theme() -> Result<(), Error> {
        let html = convert_string(
            SOURCE_BLOCK,
            &[(
                "syntect-style",
                AttributeValue::String("base16-ocean.dark".into()),
            )],
        )?;
        // With a dark theme the background / colours will differ from default light.
        // Just verify it still produces highlighted output without errors.
        assert!(
            html.contains("<span"),
            "Should produce highlighted spans with custom theme:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn class_mode_with_custom_theme_embeds_that_theme_css() -> Result<(), Error> {
        let html = convert_string(
            SOURCE_BLOCK,
            &[
                ("syntect-css", AttributeValue::String("class".into())),
                (
                    "syntect-style",
                    AttributeValue::String("Solarized (dark)".into()),
                ),
            ],
        )?;
        // The CSS should be present and the code should have class spans
        assert!(html.contains(".syntax-"), "CSS rules should be in head");
        assert!(
            html.contains("class=\"syntax-"),
            "Code should have class= spans"
        );
        Ok(())
    }

    #[test]
    fn class_mode_with_linkcss_links_stylesheet() -> Result<(), Error> {
        let html = convert_string(
            SOURCE_BLOCK,
            &[
                ("syntect-css", AttributeValue::String("class".into())),
                ("linkcss", AttributeValue::Bool(true)),
            ],
        )?;
        // Should link to the external stylesheet, not embed it
        assert!(
            html.contains(r#"<link rel="stylesheet" href="./acdc-syntect.css">"#),
            "Should link to acdc-syntect.css:\n{html}"
        );
        // Should NOT embed the CSS rules in the page
        assert!(
            !html.contains("<style>\n.syntax-"),
            "Should not embed syntax CSS when linkcss is set:\n{html}"
        );
        // Code should still have class-based spans
        assert!(
            html.contains("class=\"syntax-"),
            "Code should still have class= spans:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn class_mode_with_linkcss_and_stylesdir() -> Result<(), Error> {
        let html = convert_string(
            SOURCE_BLOCK,
            &[
                ("syntect-css", AttributeValue::String("class".into())),
                ("linkcss", AttributeValue::Bool(true)),
                ("stylesdir", AttributeValue::String("css".into())),
            ],
        )?;
        assert!(
            html.contains(r#"<link rel="stylesheet" href="css/acdc-syntect.css">"#),
            "Should link to css/acdc-syntect.css:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn inline_mode_with_linkcss_no_syntax_link() -> Result<(), Error> {
        let html = convert_string(SOURCE_BLOCK, &[("linkcss", AttributeValue::Bool(true))])?;
        // Inline mode should not link to acdc-syntect.css
        assert!(
            !html.contains("acdc-syntect.css"),
            "Inline mode should not reference acdc-syntect.css:\n{html}"
        );
        Ok(())
    }
}

mod stylesheet_modes {
    use super::*;

    const BASIC_DOC: &str = "= Title\n\nHello world.\n";

    #[test]
    fn no_stylesheet_mode_suppresses_css_and_fonts() -> Result<(), Error> {
        let html = convert_string(":!stylesheet:\n\nHello world.\n", &[])?;
        // No embedded <style> for the main stylesheet
        assert!(
            !html.contains("<style>"),
            "no-stylesheet mode should not contain <style>:\n{html}"
        );
        // No linked stylesheet
        assert!(
            !html.contains(r#"<link rel="stylesheet""#),
            "no-stylesheet mode should not contain stylesheet <link>:\n{html}"
        );
        // No Google Fonts link
        assert!(
            !html.contains("fonts.googleapis.com"),
            "no-stylesheet mode should not contain Google Fonts link:\n{html}"
        );
        // Body content should still be present
        assert!(
            html.contains("Hello world."),
            "content should still be rendered"
        );
        Ok(())
    }

    #[test]
    fn no_stylesheet_mode_preserves_mathjax() -> Result<(), Error> {
        let html = convert_string(":!stylesheet:\n:stem:\n\nHello world.\n", &[])?;
        assert!(
            html.contains("MathJax"),
            "no-stylesheet mode should still include MathJax when :stem: is set:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn no_stylesheet_mode_preserves_font_awesome() -> Result<(), Error> {
        let html = convert_string(":!stylesheet:\n:icons: font\n\nHello world.\n", &[])?;
        assert!(
            html.contains("fontawesome"),
            "no-stylesheet mode should still include Font Awesome when :icons: font is set:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn default_mode_includes_embedded_css() -> Result<(), Error> {
        let html = convert_string(BASIC_DOC, &[])?;
        assert!(
            html.contains("<style>"),
            "default mode should embed CSS in <style>:\n{html}"
        );
        assert!(
            html.contains("fonts.googleapis.com"),
            "default mode should include Google Fonts link:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn linkcss_mode_links_stylesheet() -> Result<(), Error> {
        let html = convert_string(BASIC_DOC, &[("linkcss", AttributeValue::Bool(true))])?;
        assert!(
            html.contains(r#"<link rel="stylesheet" href="./"#),
            "linkcss mode should link to stylesheet:\n{html}"
        );
        // Should still have supplementary stem styles
        assert!(
            html.contains(".stemblock .content"),
            "linkcss mode should include supplementary stem styles:\n{html}"
        );
        Ok(())
    }
}

mod webfonts {
    use super::*;

    #[test]
    fn default_includes_google_fonts() -> Result<(), Error> {
        let html = convert_string("= Title\n\nHello.\n", &[])?;
        assert!(
            html.contains("fonts.googleapis.com/css?family=Open+Sans"),
            "default should include Open Sans font link:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn webfonts_disabled_suppresses_font_link() -> Result<(), Error> {
        let html = convert_string(":!webfonts:\n\nHello.\n", &[])?;
        // No Google Fonts <link> tag (the CSS content itself may mention fonts in comments)
        assert!(
            !html.contains(r#"<link rel="stylesheet" href="https://fonts.googleapis.com"#),
            ":!webfonts: should suppress Google Fonts <link> tag:\n{html}"
        );
        // Should still have stylesheet
        assert!(
            html.contains("<style>"),
            "disabling webfonts should not affect stylesheet:\n{html}"
        );
        Ok(())
    }

    #[test]
    fn webfonts_custom_value_uses_custom_url() -> Result<(), Error> {
        let html = convert_string(":webfonts: Roboto:400,700\n\nHello.\n", &[])?;
        assert!(
            html.contains(r#"<link rel="stylesheet" href="https://fonts.googleapis.com/css?family=Roboto:400,700">"#),
            "custom :webfonts: value should appear in font <link> tag:\n{html}"
        );
        // The default Open Sans font <link> should not be present
        assert!(
            !html.contains(
                r#"<link rel="stylesheet" href="https://fonts.googleapis.com/css?family=Open+Sans"#
            ),
            "custom :webfonts: should replace default Open Sans <link> tag:\n{html}"
        );
        Ok(())
    }
}

mod copycss {
    use super::*;
    use acdc_converters_core::Converter;

    #[test]
    fn linkcss_with_default_stylesheet_writes_builtin_css() -> Result<(), Error> {
        let tmp = tempfile::tempdir()?;
        let html_path = tmp.path().join("output.html");

        let input = "= Title\n:linkcss:\n\nHello.\n";
        let mut attrs = acdc_converters_core::default_rendering_attributes();
        attrs.insert("linkcss".into(), AttributeValue::Bool(true));
        attrs.insert("copycss".into(), AttributeValue::String(String::new()));

        let parser_options = ParserOptions::with_attributes(attrs);
        let doc = acdc_parser::parse(input, &parser_options)?;

        let converter_options = ConverterOptions::builder()
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .backend(Backend::Html)
            .build();
        let processor = Processor::new_with_variant(
            converter_options,
            doc.attributes.clone(),
            HtmlVariant::Standard,
        );

        // Write HTML output
        let mut html_output = Vec::new();
        processor.convert_to_writer(&doc, &mut html_output, &RenderOptions::default())?;
        std::fs::write(&html_path, &html_output)?;

        // Trigger after_write to copy CSS
        processor.after_write(&doc, &html_path);

        // The built-in stylesheet should have been written to disk
        let css_path = tmp.path().join("asciidoctor-light-mode.css");
        assert!(
            css_path.exists(),
            "built-in stylesheet should be written to disk at {}",
            css_path.display()
        );

        let css_content = std::fs::read_to_string(&css_path)?;
        assert!(
            !css_content.is_empty(),
            "written CSS file should not be empty"
        );

        Ok(())
    }

    #[test]
    fn copycss_value_used_as_source_path() -> Result<(), Error> {
        let tmp = tempfile::tempdir()?;
        let html_path = tmp.path().join("output.html");

        // Create a custom CSS file to be used as copycss source
        let custom_css_path = tmp.path().join("my-custom.css");
        std::fs::write(&custom_css_path, "body { color: red; }")?;

        let input = "= Title\n:linkcss:\n:stylesheet: target.css\n\nHello.\n";
        let mut attrs = acdc_converters_core::default_rendering_attributes();
        attrs.insert("linkcss".into(), AttributeValue::Bool(true));
        attrs.insert(
            "copycss".into(),
            AttributeValue::String(custom_css_path.to_string_lossy().into()),
        );
        attrs.insert(
            "stylesheet".into(),
            AttributeValue::String("target.css".into()),
        );

        let parser_options = ParserOptions::with_attributes(attrs);
        let doc = acdc_parser::parse(input, &parser_options)?;

        let converter_options = ConverterOptions::builder()
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .backend(Backend::Html)
            .build();
        let processor = Processor::new_with_variant(
            converter_options,
            doc.attributes.clone(),
            HtmlVariant::Standard,
        );

        // Write HTML output
        let mut html_output = Vec::new();
        processor.convert_to_writer(&doc, &mut html_output, &RenderOptions::default())?;
        std::fs::write(&html_path, &html_output)?;

        // Trigger after_write
        processor.after_write(&doc, &html_path);

        // The custom CSS should have been copied to target.css
        let target_path = tmp.path().join("target.css");
        assert!(
            target_path.exists(),
            "copycss source should be copied to target path at {}",
            target_path.display()
        );

        let content = std::fs::read_to_string(&target_path)?;
        assert_eq!(
            content, "body { color: red; }",
            "copied file should have the custom CSS content"
        );

        Ok(())
    }

    #[test]
    fn no_stylesheet_mode_skips_copycss() -> Result<(), Error> {
        let tmp = tempfile::tempdir()?;
        let html_path = tmp.path().join("output.html");

        let input = ":!stylesheet:\n:linkcss:\n\nHello.\n";
        let mut attrs = acdc_converters_core::default_rendering_attributes();
        attrs.insert("stylesheet".into(), AttributeValue::Bool(false));
        attrs.insert("linkcss".into(), AttributeValue::Bool(true));
        attrs.insert("copycss".into(), AttributeValue::String(String::new()));

        let parser_options = ParserOptions::with_attributes(attrs);
        let doc = acdc_parser::parse(input, &parser_options)?;

        let converter_options = ConverterOptions::builder()
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .backend(Backend::Html)
            .build();
        let processor = Processor::new_with_variant(
            converter_options,
            doc.attributes.clone(),
            HtmlVariant::Standard,
        );

        std::fs::write(&html_path, "dummy")?;
        processor.after_write(&doc, &html_path);

        // No CSS files should be written
        let css_files: Vec<_> = std::fs::read_dir(tmp.path())?
            .filter_map(std::result::Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "css"))
            .collect();
        assert!(
            css_files.is_empty(),
            "no CSS files should be written in no-stylesheet mode"
        );

        Ok(())
    }

    #[test]
    fn embedded_mode_skips_copycss() -> Result<(), Error> {
        let tmp = tempfile::tempdir()?;
        let html_path = tmp.path().join("output.html");

        let input = "= Title\n:linkcss:\n\nHello.\n";
        let mut attrs = acdc_converters_core::default_rendering_attributes();
        attrs.insert("linkcss".into(), AttributeValue::Bool(true));
        attrs.insert("copycss".into(), AttributeValue::String(String::new()));

        let parser_options = ParserOptions::with_attributes(attrs);
        let doc = acdc_parser::parse(input, &parser_options)?;

        let converter_options = ConverterOptions::builder()
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .backend(Backend::Html)
            .embedded(true)
            .build();
        let processor = Processor::new_with_variant(
            converter_options,
            doc.attributes.clone(),
            HtmlVariant::Standard,
        );

        std::fs::write(&html_path, "dummy")?;
        processor.after_write(&doc, &html_path);

        // No CSS files should be written in embedded mode
        let css_files: Vec<_> = std::fs::read_dir(tmp.path())?
            .filter_map(std::result::Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "css"))
            .collect();
        assert!(
            css_files.is_empty(),
            "no CSS files should be written in embedded mode, found: {:?}",
            css_files
                .iter()
                .map(std::fs::DirEntry::path)
                .collect::<Vec<_>>()
        );

        Ok(())
    }
}

mod docinfo {
    use super::*;

    /// Helper: create a temp dir with an `.adoc` source file and optional docinfo files,
    /// parse it, and return the converted HTML string.
    fn convert_with_docinfo(
        adoc_content: &str,
        docinfo_files: &[(&str, &str)],
        embedded: bool,
        safe_mode: SafeMode,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        let adoc_path = tmp.path().join("mydoc.adoc");
        std::fs::write(&adoc_path, adoc_content)?;

        for (name, content) in docinfo_files {
            std::fs::write(tmp.path().join(name), content)?;
        }

        let parser_options = ParserOptions::default();
        let doc = acdc_parser::parse_file(&adoc_path, &parser_options)?;

        let converter_options = ConverterOptions::builder()
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .safe_mode(safe_mode)
            .build();
        let processor = Processor::new_with_variant(
            converter_options,
            doc.attributes.clone(),
            HtmlVariant::Standard,
        );
        let render_options = RenderOptions {
            embedded,
            source_dir: Some(tmp.path().to_path_buf()),
            docname: Some("mydoc".to_string()),
            ..RenderOptions::default()
        };

        let html = processor.convert_to_string(&doc, &render_options)?;
        Ok(html)
    }

    #[test]
    fn shared_head_docinfo_injected() -> Result<(), Box<dyn std::error::Error>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: shared\n\nHello.\n",
            &[("docinfo.html", "<style>.custom { color: red; }</style>")],
            false,
            SafeMode::Unsafe,
        )?;

        assert!(
            html.contains("<style>.custom { color: red; }</style>"),
            "shared head docinfo content should be in output"
        );
        // Content should appear before </head>
        let before_head_close = html.split("</head>").next().unwrap_or("");
        assert!(
            before_head_close.contains("<style>.custom { color: red; }</style>"),
            "docinfo head content should appear before </head>"
        );
        Ok(())
    }

    #[test]
    fn private_head_docinfo_injected() -> Result<(), Box<dyn std::error::Error>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: private\n\nHello.\n",
            &[(
                "mydoc-docinfo.html",
                "<meta name=\"custom\" content=\"value\">",
            )],
            false,
            SafeMode::Unsafe,
        )?;

        assert!(
            html.contains("<meta name=\"custom\" content=\"value\">"),
            "private head docinfo content should be in output"
        );
        Ok(())
    }

    #[test]
    fn shared_header_docinfo_injected() -> Result<(), Box<dyn std::error::Error>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: shared\n\nHello.\n",
            &[(
                "docinfo-header.html",
                "<div id=\"custom-banner\">Banner</div>",
            )],
            false,
            SafeMode::Unsafe,
        )?;

        assert!(
            html.contains("<div id=\"custom-banner\">Banner</div>"),
            "shared header docinfo content should be in output"
        );
        // Content should appear after <body...>
        let after_body_open = html.split("<body").nth(1).unwrap_or("");
        assert!(
            after_body_open.contains("<div id=\"custom-banner\">Banner</div>"),
            "docinfo header content should appear after <body>"
        );
        Ok(())
    }

    #[test]
    fn private_header_docinfo_injected() -> Result<(), Box<dyn std::error::Error>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: private\n\nHello.\n",
            &[(
                "mydoc-docinfo-header.html",
                "<nav id=\"private-nav\">Nav</nav>",
            )],
            false,
            SafeMode::Unsafe,
        )?;

        assert!(
            html.contains("<nav id=\"private-nav\">Nav</nav>"),
            "private header docinfo content should be in output"
        );
        Ok(())
    }

    #[test]
    fn shared_footer_docinfo_injected() -> Result<(), Box<dyn std::error::Error>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: shared\n\nHello.\n",
            &[(
                "docinfo-footer.html",
                "<script>console.log('analytics');</script>",
            )],
            false,
            SafeMode::Unsafe,
        )?;

        assert!(
            html.contains("<script>console.log('analytics');</script>"),
            "shared footer docinfo content should be in output"
        );
        // Content should appear before </body>
        let before_body_close = html.split("</body>").next().unwrap_or("");
        assert!(
            before_body_close.contains("<script>console.log('analytics');</script>"),
            "docinfo footer content should appear before </body>"
        );
        Ok(())
    }

    #[test]
    fn private_footer_docinfo_injected() -> Result<(), Box<dyn std::error::Error>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: private\n\nHello.\n",
            &[(
                "mydoc-docinfo-footer.html",
                "<div id=\"private-footer\">PF</div>",
            )],
            false,
            SafeMode::Unsafe,
        )?;

        assert!(
            html.contains("<div id=\"private-footer\">PF</div>"),
            "private footer docinfo content should be in output"
        );
        Ok(())
    }

    #[test]
    fn combined_shared_and_private() -> Result<(), Box<dyn std::error::Error>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: shared,private\n\nHello.\n",
            &[
                ("docinfo.html", "<!-- shared-head -->"),
                ("mydoc-docinfo.html", "<!-- private-head -->"),
                ("docinfo-header.html", "<!-- shared-header -->"),
                ("mydoc-docinfo-header.html", "<!-- private-header -->"),
                ("docinfo-footer.html", "<!-- shared-footer -->"),
                ("mydoc-docinfo-footer.html", "<!-- private-footer -->"),
            ],
            false,
            SafeMode::Unsafe,
        )?;

        // All six should be present
        assert!(html.contains("<!-- shared-head -->"), "shared head missing");
        assert!(
            html.contains("<!-- private-head -->"),
            "private head missing"
        );
        assert!(
            html.contains("<!-- shared-header -->"),
            "shared header missing"
        );
        assert!(
            html.contains("<!-- private-header -->"),
            "private header missing"
        );
        assert!(
            html.contains("<!-- shared-footer -->"),
            "shared footer missing"
        );
        assert!(
            html.contains("<!-- private-footer -->"),
            "private footer missing"
        );

        // Private should appear before shared (within each position)
        let before_shared = html.split("<!-- shared-head -->").next().unwrap_or("");
        assert!(
            before_shared.contains("<!-- private-head -->"),
            "private head should come before shared head"
        );

        Ok(())
    }

    #[test]
    fn embedded_mode_skips_docinfo() -> Result<(), Box<dyn std::error::Error>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: shared\n\nHello.\n",
            &[
                ("docinfo.html", "<!-- should-not-appear -->"),
                ("docinfo-footer.html", "<!-- also-hidden -->"),
            ],
            true,
            SafeMode::Unsafe,
        )?;

        assert!(
            !html.contains("<!-- should-not-appear -->"),
            "docinfo head should not appear in embedded mode"
        );
        assert!(
            !html.contains("<!-- also-hidden -->"),
            "docinfo footer should not appear in embedded mode"
        );
        Ok(())
    }

    #[test]
    fn missing_file_no_error() -> Result<(), Box<dyn std::error::Error>> {
        // No docinfo files present, should not error
        let html = convert_with_docinfo(
            "= Title\n:docinfo: shared\n\nHello.\n",
            &[],
            false,
            SafeMode::Unsafe,
        )?;

        // Should still produce valid HTML
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("</html>"));
        Ok(())
    }

    #[test]
    fn docinfodir_overrides_source_dir() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;

        // Create source file in root
        let adoc_path = tmp.path().join("mydoc.adoc");
        std::fs::write(
            &adoc_path,
            "= Title\n:docinfo: shared\n:docinfodir: custom-docinfo\n\nHello.\n",
        )?;

        // Create docinfo in a subdirectory
        let docinfo_dir = tmp.path().join("custom-docinfo");
        std::fs::create_dir(&docinfo_dir)?;
        std::fs::write(docinfo_dir.join("docinfo.html"), "<!-- from-custom-dir -->")?;

        // Also create one in source dir (should NOT be picked up)
        std::fs::write(tmp.path().join("docinfo.html"), "<!-- from-source-dir -->")?;

        let parser_options = ParserOptions::default();
        let doc = acdc_parser::parse_file(&adoc_path, &parser_options)?;

        let converter_options = ConverterOptions::builder()
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .build();
        let processor = Processor::new_with_variant(
            converter_options,
            doc.attributes.clone(),
            HtmlVariant::Standard,
        );
        let render_options = RenderOptions {
            source_dir: Some(tmp.path().to_path_buf()),
            docname: Some("mydoc".to_string()),
            ..RenderOptions::default()
        };

        let html = processor.convert_to_string(&doc, &render_options)?;

        assert!(
            html.contains("<!-- from-custom-dir -->"),
            "should use docinfo from docinfodir"
        );
        assert!(
            !html.contains("<!-- from-source-dir -->"),
            "should not use docinfo from source dir when docinfodir is set"
        );
        Ok(())
    }

    #[test]
    fn secure_safe_mode_disables_docinfo() -> Result<(), Box<dyn std::error::Error>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: shared\n\nHello.\n",
            &[("docinfo.html", "<!-- secret-content -->")],
            false,
            SafeMode::Secure,
        )?;

        assert!(
            !html.contains("<!-- secret-content -->"),
            "docinfo should not appear in secure safe mode"
        );
        Ok(())
    }

    #[test]
    fn attribute_substitution_in_docinfo() -> Result<(), Box<dyn std::error::Error>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: shared\n:my-custom-attr: replaced-value\n\nHello.\n",
            &[(
                "docinfo.html",
                "<meta name=\"custom\" content=\"{my-custom-attr}\">",
            )],
            false,
            SafeMode::Unsafe,
        )?;

        assert!(
            html.contains("<meta name=\"custom\" content=\"replaced-value\">"),
            "attribute references in docinfo should be substituted"
        );
        assert!(
            !html.contains("{my-custom-attr}"),
            "raw attribute reference should not remain"
        );
        Ok(())
    }

    #[test]
    fn docinfo_bare_attribute_defaults_to_private() -> Result<(), Box<dyn std::error::Error>> {
        // `:docinfo:` with no value should default to "private"
        let html = convert_with_docinfo(
            "= Title\n:docinfo:\n\nHello.\n",
            &[("mydoc-docinfo.html", "<!-- private-default -->")],
            false,
            SafeMode::Unsafe,
        )?;

        assert!(
            html.contains("<!-- private-default -->"),
            "bare :docinfo: should default to private scope"
        );
        Ok(())
    }

    #[test]
    fn granular_shared_head_only() -> Result<(), Box<dyn std::error::Error>> {
        let html = convert_with_docinfo(
            "= Title\n:docinfo: shared-head\n\nHello.\n",
            &[
                ("docinfo.html", "<!-- head-content -->"),
                ("docinfo-footer.html", "<!-- footer-should-not-appear -->"),
            ],
            false,
            SafeMode::Unsafe,
        )?;

        assert!(
            html.contains("<!-- head-content -->"),
            "shared-head docinfo should appear"
        );
        assert!(
            !html.contains("<!-- footer-should-not-appear -->"),
            "footer docinfo should not appear when only shared-head is set"
        );
        Ok(())
    }
}
