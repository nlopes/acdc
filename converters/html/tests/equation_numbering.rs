use acdc_converters_core::Options;
use acdc_converters_html::{HtmlVariant, MATHJAX_LOADER_URL, Processor, RenderOptions};
use acdc_parser::{CompatibilityMode, Document, Options as ParserOptions, parse};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("", None)]
#[case(":eqnums:\n", Some("ams"))]
#[case(":eqnums: AMS\n", Some("ams"))]
#[case(":eqnums: all\n", Some("all"))]
#[case(":eqnums: none\n", None)]
#[case(":eqnums: all\n:eqnums!:\n", None)]
#[case(":eqnums: unsupported\n", None)]
#[case(":eqnums: </script><script>alert('bad')</script>\n", None)]
fn equation_numbering_selects_safe_configuration_and_matching_csp(
    #[case] setting: &str,
    #[case] tags: Option<&str>,
    #[values(HtmlVariant::Standard, HtmlVariant::Semantic)] variant: HtmlVariant,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(false, true)] embedded: bool,
) -> Result<(), Error> {
    let source = format!(
        "= Equations\n:stem: latexmath\n:csp:\n:stylesheet!:\n:webfonts!:\n{setting}\nInline latexmath:[x + 1].\n\n[latexmath]\n++++\nx = 2\n++++\n"
    );
    let parsed = parse(
        &source,
        &ParserOptions::builder()
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let before = Document::from(parsed.document());
    let processor = Processor::new_with_variant(
        Options::default(),
        ParserOptions::builder().with_compatibility_mode(mode),
        variant,
    )?;
    let output = processor.convert_to_string(
        parsed.document(),
        &RenderOptions {
            embedded,
            ..RenderOptions::default()
        },
    )?;
    assert_eq!(Document::from(parsed.document()), before);
    assert!(output.contains(r"\(x + 1\)"));
    assert!(output.contains(r"\[x = 2\]"));
    assert!(!output.contains("alert('bad')"));
    assert_eq!(output.contains(MATHJAX_LOADER_URL), !embedded);
    if embedded {
        assert!(!output.contains("<script"));
    } else {
        let scripts = output
            .split("<script>")
            .skip(1)
            .map(|part| part.split_once("</script>").map(|(script, _)| script))
            .collect::<Option<Vec<_>>>()
            .ok_or("unclosed script")?;
        assert_eq!(scripts.len(), 1 + usize::from(tags.is_some()));
        if let Some(tags) = tags {
            assert!(output.contains(&format!("MathJax.tex.tags = '{tags}';")));
        }
        let csp = output
            .lines()
            .find(|line| line.contains("http-equiv=\"Content-Security-Policy\""))
            .ok_or("missing CSP")?;
        for script in scripts {
            let hash = format!("'sha256-{}'", STANDARD.encode(Sha256::digest(script)));
            assert!(
                csp.contains(&hash),
                "CSP does not allow emitted script: {hash}"
            );
        }
        let script_policy = csp.split("style-src").next().ok_or("script policy")?;
        assert!(!script_policy.contains("'unsafe-inline'"));
    }
    Ok(())
}

#[rstest::rstest]
fn equation_numbering_requires_stem_and_uses_the_header_setting(
    #[values(HtmlVariant::Standard, HtmlVariant::Semantic)] variant: HtmlVariant,
) -> Result<(), Error> {
    for (header, body, expected) in [
        (":eqnums: all\n", "", None),
        (":stem:\n", ":eqnums: all", None),
        (":stem:\n:eqnums: all\n", ":eqnums!:", Some("all")),
        (
            ":stem:\n",
            "[cols=\"a\"]\n|===\n|\n:eqnums: all\n\nCell\n|===",
            None,
        ),
    ] {
        let source =
            format!("= Equations\n:csp:\n:stylesheet!:\n:webfonts!:\n{header}\nBody.\n\n{body}\n");
        let parsed = parse(&source, &ParserOptions::default())?;
        let processor =
            Processor::new_with_variant(Options::default(), ParserOptions::builder(), variant)?;
        let output = processor.convert_to_string(parsed.document(), &RenderOptions::default())?;
        assert_eq!(output.contains("MathJax.tex.tags ="), expected.is_some());
        if let Some(tags) = expected {
            assert!(output.contains(&format!("MathJax.tex.tags = '{tags}';")));
        }
    }
    Ok(())
}
