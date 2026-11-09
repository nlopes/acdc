use acdc_converters_core::Options;
use acdc_converters_html::{Converter, HtmlVariant, RenderOptions};
use acdc_parser::{CompatibilityMode, Document, Options as ParserOptions, parse, parse_file};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("", "", ".html")]
#[case(":relfileprefix: manual/\n", "manual/", ".html")]
#[case(":relfilesuffix: .xhtml\n", "", ".xhtml")]
#[case(
    ":relfileprefix: manual/\n:relfilesuffix: .xhtml\n",
    "manual/",
    ".xhtml"
)]
#[case(":relfileprefix:\n:relfilesuffix:\n", "", "")]
#[case(":relfilesuffix: html\n", "", "html")]
#[case(
    ":relfileprefix: manual/\n:relfileprefix!:\n:relfilesuffix: .xhtml\n:relfilesuffix!:\n",
    "",
    ".html"
)]
fn file_controls_preserve_ast_targets_and_local_anchors(
    #[case] settings: &str,
    #[case] prefix: &str,
    #[case] suffix: &str,
    #[values(false, true)] force_local: bool,
    #[values(HtmlVariant::Standard, HtmlVariant::Semantic)] variant: HtmlVariant,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!(
        "= File controls\n:stylesheet!:\n:webfonts!:\n{settings}{}\n[[local]]\n== Local\n\nxref:#local[Local label] xref:#local[]\n\nxref:guide.adoc[File] xref:guide.adoc[]\n\nxref:guide.adoc#part[Part] xref:guide.adoc#part[]\n\nxref:guide#part[Bare]\n\nxref:guide.pdf#part[PDF]\n\nxref:https://example.org/guide.adoc#part[Remote]\n\nxref://example.org/guide.adoc#part[Network]\n\nxref:mailto:a@example.org[Mail]\n",
        if force_local {
            ":force-local-file-xrefs:\n"
        } else {
            ""
        }
    );
    let parsed = parse(
        &source,
        &ParserOptions::builder()
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let before = Document::from(parsed.document());
    let converter = Converter::new_with_variant(
        Options::default(),
        ParserOptions::builder().with_compatibility_mode(mode),
        variant,
    )?;
    let output = converter.convert_to_string(
        parsed.document(),
        &RenderOptions {
            embedded: true,
            ..RenderOptions::default()
        },
    )?;
    assert_eq!(Document::from(parsed.document()), before);
    for expected in [
        "href=\"#local\">Local label</a>".to_string(),
        "href=\"#local\">Local</a>".to_string(),
        format!("href=\"{prefix}guide{suffix}\">File</a>"),
        format!("href=\"{prefix}guide{suffix}\">{prefix}guide{suffix}</a>"),
        format!("href=\"{prefix}guide{suffix}#part\">Part</a>"),
        format!("href=\"{prefix}guide{suffix}#part\">{prefix}guide{suffix}</a>"),
        format!("href=\"{prefix}guide{suffix}#part\">Bare</a>"),
        format!("href=\"{prefix}guide.pdf#part\">PDF</a>"),
        format!("href=\"https://example.org/guide{suffix}#part\">Remote</a>"),
        format!("href=\"//example.org/guide{suffix}#part\">Network</a>"),
        "href=\"mailto:a@example.org\">Mail</a>".to_string(),
    ] {
        assert!(output.contains(&expected), "missing {expected}: {output}");
    }
    Ok(())
}

#[rstest::rstest]
fn file_controls_keep_included_references_local(
    #[values(false, true)] force_local: bool,
    #[values(HtmlVariant::Standard, HtmlVariant::Semantic)] variant: HtmlVariant,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    std::fs::write(
        directory.path().join("part.adoc"),
        "[[target]]\n== Included\n",
    )?;
    let source = directory.path().join("document.adoc");
    std::fs::write(
        &source,
        format!(
            "= Includes\n:relfileprefix: manual/\n:relfilesuffix: .xhtml\n{}\nxref:part.adoc#target[Before]\n\ninclude::part.adoc[]\n\nxref:part.adoc#target[After]\n",
            if force_local {
                ":force-local-file-xrefs:\n"
            } else {
                ""
            }
        ),
    )?;
    let parsed = parse_file(
        &source,
        &ParserOptions::builder()
            .with_safe_mode(acdc_parser::SafeMode::Safe)
            .build()?,
    )?;
    let before = Document::from(parsed.document());
    let converter =
        Converter::new_with_variant(Options::default(), ParserOptions::builder(), variant)?;
    let output = converter.convert_to_string(parsed.document(), &RenderOptions::default())?;
    assert_eq!(Document::from(parsed.document()), before);
    assert!(output.contains("href=\"#target\">Before</a>"));
    assert!(output.contains("href=\"#target\">After</a>"));
    assert!(!output.contains("manual/part"));
    Ok(())
}

#[rstest::rstest]
fn file_controls_follow_body_assignments_and_preserve_cell_inheritance(
    #[values(HtmlVariant::Standard, HtmlVariant::Semantic)] variant: HtmlVariant,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Scoped files\n:stylesheet!:\n:webfonts!:\n:relfileprefix: root/\n:relfilesuffix: .xhtml\n\nxref:guide.adoc[Header]\n\n:relfileprefix: body/\n\nxref:guide.adoc[Body]\n\n[cols=\"a\"]\n|===\n|\n:relfileprefix: cell/\n:relfilesuffix:\n\nxref:guide.adoc[Cell]\n|===\n\nxref:guide.adoc[After]\n\n:relfileprefix!:\n:relfilesuffix!:\n\nxref:guide.adoc[Unset]\n",
        &ParserOptions::builder()
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let converter = Converter::new_with_variant(
        Options::default(),
        ParserOptions::builder().with_compatibility_mode(mode),
        variant,
    )?;
    let output = converter.convert_to_string(parsed.document(), &RenderOptions::default())?;
    for (target, label) in [
        ("root/guide.xhtml", "Header"),
        ("body/guide.xhtml", "Body"),
        ("body/guide.xhtml", "Cell"),
        ("body/guide.xhtml", "After"),
        ("guide.html", "Unset"),
    ] {
        let expected = format!("href=\"{target}\">{label}</a>");
        assert!(output.contains(&expected), "missing {expected}: {output}");
    }
    Ok(())
}

#[rstest::rstest]
fn file_prefix_and_suffix_cannot_escape_the_link_attribute(
    #[values(HtmlVariant::Standard, HtmlVariant::Semantic)] variant: HtmlVariant,
) -> Result<(), Error> {
    let parsed = parse(
        "= Escaped files\n:relfileprefix: docs/\"&/\n:relfilesuffix: .html?x=\"&\n\nxref:guide.adoc[File]\n",
        &ParserOptions::default(),
    )?;
    let converter =
        Converter::new_with_variant(Options::default(), ParserOptions::builder(), variant)?;
    let output = converter.convert_to_string(parsed.document(), &RenderOptions::default())?;
    assert!(output.contains("href=\"docs/&quot;&amp;/guide.html?x=&quot;&amp;\">File</a>"));
    Ok(())
}
