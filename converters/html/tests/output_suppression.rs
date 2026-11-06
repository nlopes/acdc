use acdc_converters_core::Options;
use acdc_converters_html::{HtmlVariant, Processor, RenderOptions};
use acdc_parser::{CompatibilityMode, Document, Options as ParserOptions, parse};

type Error = Box<dyn std::error::Error>;

fn convert(
    source: &str,
    mode: CompatibilityMode,
    variant: HtmlVariant,
    embedded: bool,
) -> Result<String, Error> {
    let parsed = parse(
        source,
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
    Ok(output)
}

fn has_header(output: &str) -> bool {
    output.contains("id=\"header\"") || output.contains("<header>")
}

fn has_footnotes(output: &str) -> bool {
    output.contains("id=\"footnotes\"") || output.contains("role=\"doc-endnotes\"")
}

#[rstest::rstest]
fn suppression_controls_preserve_content_and_metadata(
    #[values("noheader", "nofooter", "nofootnotes")] control: &str,
    #[values("absent", "set", "empty", "unset")] state: &str,
    #[values(false, true)] embedded: bool,
    #[values(HtmlVariant::Standard, HtmlVariant::Semantic)] variant: HtmlVariant,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let setting = match state {
        "set" => format!(":{control}: yes\n"),
        "empty" => format!(":{control}:\n"),
        "unset" => format!(":{control}:\n:{control}!:\n"),
        _ => String::new(),
    };
    let source = format!(
        "= Visible title\nAuthor Name\nv1.0, 2026-11-06\n:stylesheet!:\n:webfonts!:\n:toc:\n{setting}\nBody footnote:[A note.].\n\n== Section\n\nContent.\n"
    );
    let output = convert(&source, mode, variant, embedded)?;
    let suppressed = matches!(state, "set" | "empty");
    assert_eq!(
        has_header(&output),
        !(embedded || control == "noheader" && suppressed)
    );
    assert_eq!(
        output.contains("id=\"footer\""),
        !(embedded || control == "nofooter" && suppressed)
    );
    assert_eq!(
        has_footnotes(&output),
        !(control == "nofootnotes" && suppressed)
    );
    assert_eq!(
        output.contains("id=\"toc\""),
        embedded || !(control == "noheader" && suppressed)
    );
    assert_eq!(output.contains("<title>Visible title</title>"), !embedded);
    assert_eq!(output.contains("<meta name=\"author\""), !embedded);
    assert!(output.contains("id=\"_footnoteref_1\""));
    assert!(output.contains("Content."));
    Ok(())
}

#[rstest::rstest]
fn suppression_follows_body_assignments_at_the_rendered_position(
    #[values(HtmlVariant::Standard, HtmlVariant::Semantic)] variant: HtmlVariant,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    for (header, body, notes, footer) in [
        ("", ":noheader:\n:nofooter:\n:nofootnotes:", false, false),
        (
            ":nofooter:\n:nofootnotes:\n",
            ":nofooter!:\n:nofootnotes!:",
            true,
            true,
        ),
        (
            "",
            ":nofooter:\n:nofootnotes:\n\n:nofooter!:\n:nofootnotes!:",
            true,
            true,
        ),
    ] {
        let source = format!(
            "= Body settings\n:stylesheet!:\n:webfonts!:\n{header}\nA footnote:[Note.].\n\n{body}\n\nAfter.\n"
        );
        let output = convert(&source, mode, variant, false)?;
        assert!(has_header(&output));
        assert_eq!(has_footnotes(&output), notes);
        assert_eq!(output.contains("id=\"footer\""), footer);
    }
    Ok(())
}

#[rstest::rstest]
fn noheader_controls_headerless_toc_wrappers(
    #[values(false, true)] embedded: bool,
    #[values(HtmlVariant::Standard, HtmlVariant::Semantic)] variant: HtmlVariant,
) -> Result<(), Error> {
    for (setting, suppressed) in [(":noheader:", true), (":noheader!:", false)] {
        let source =
            format!(":stylesheet!:\n:webfonts!:\n:toc:\n{setting}\n\n== Section\n\nContent.\n");
        let output = convert(&source, CompatibilityMode::Asciidoctor, variant, embedded)?;
        assert_eq!(has_header(&output), !embedded && !suppressed);
        assert_eq!(output.contains("id=\"toc\""), embedded || !suppressed);
        assert!(output.contains("Content."));
    }
    Ok(())
}

#[rstest::rstest]
fn cell_suppression_does_not_escape_to_the_document(
    #[values(HtmlVariant::Standard, HtmlVariant::Semantic)] variant: HtmlVariant,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "= Cell settings\n:stylesheet!:\n:webfonts!:\n\nA footnote:[Outer note.].\n\n[cols=\"a\"]\n|===\n|\n:nofooter:\n:nofootnotes:\n\nCell content.\n|===\n";
    let output = convert(source, mode, variant, false)?;
    assert!(has_header(&output));
    assert!(has_footnotes(&output));
    assert!(output.contains("id=\"footer\""));
    Ok(())
}

#[rstest::rstest]
fn suppressed_header_and_footer_keep_docinfo(
    #[values(HtmlVariant::Standard, HtmlVariant::Semantic)] variant: HtmlVariant,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    std::fs::write(
        directory.path().join("docinfo-header.html"),
        "<aside>Custom header</aside>",
    )?;
    std::fs::write(
        directory.path().join("docinfo-footer.html"),
        "<aside>Custom footer</aside>",
    )?;
    let parsed = parse(
        "= Docinfo\n:docinfo: shared-header,shared-footer\n:noheader:\n:nofooter:\n\nContent.\n",
        &ParserOptions::default(),
    )?;
    let processor = Processor::new_with_variant(
        Options::builder()
            .safe_mode(acdc_parser::SafeMode::Unsafe)
            .build(),
        ParserOptions::builder(),
        variant,
    )?;
    let output = processor.convert_to_string(
        parsed.document(),
        &RenderOptions {
            source_dir: Some(directory.path().to_path_buf()),
            ..RenderOptions::default()
        },
    )?;
    assert!(!has_header(&output));
    assert!(!output.contains("id=\"footer\""));
    assert!(output.contains("<aside>Custom header</aside>"));
    assert!(output.contains("<aside>Custom footer</aside>"));
    Ok(())
}
