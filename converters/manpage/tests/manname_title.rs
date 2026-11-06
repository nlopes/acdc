use acdc_converters_core::{Converter, Diagnostics, Options, WarningSource};
use acdc_converters_manpage::Processor;
use acdc_parser::{CompatibilityMode, Options as ParserOptions, parse};

#[rstest::rstest]
#[case("NAME", "", false)]
#[case("NAME", ":manname-title: NOMS\n", false)]
#[case("NAME", ":manname-title:\n", false)]
#[case("NAME", ":manname-title!:\n", false)]
#[case("NOMS", "", false)]
#[case("NOMS", ":manname-title: NOMS\n", false)]
#[case("NOMS", ":manname-title:\n", true)]
#[case("NOMS", ":manname-title!:\n", true)]
#[case("OTHER", ":manname-title: NOMS\n", true)]
fn name_section_conventions_accept_literal_or_configured_titles(
    #[case] heading: &str,
    #[case] setting: &str,
    #[case] expect_warning: bool,
    #[values(false, true)] embedded: bool,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let source = format!(
        "= names(1)\n:doctype: manpage\n:revdate: 2026-11-06\n{setting}\n== {heading}\n\nnames - name heading controls\n\n== SYNOPSIS\n\nnames\n"
    );
    let parsed = parse(
        &source,
        &ParserOptions::builder()
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let processor = Processor::new(
        Options::builder().embedded(embedded).build(),
        ParserOptions::builder().with_compatibility_mode(mode),
    )?;
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let warning_source = WarningSource::new("manpage");
    processor.write_to(
        parsed.document(),
        &mut output,
        None,
        None,
        &mut Diagnostics::new(&warning_source, &mut warnings),
    )?;
    assert_eq!(
        warnings.len(),
        usize::from(expect_warning),
        "{heading}, {setting}: {warnings:?}"
    );
    if let Some(warning) = warnings.first() {
        assert_eq!(
            warning.message,
            format!("manpage convention: name section should be first, got `{heading}`")
        );
        assert!(warning.advice().is_some());
    }
    let output = String::from_utf8(output)?;
    assert!(output.contains(".SH \"SYNOPSIS\""));
    if !embedded {
        assert!(output.contains(&format!(".SH \"{heading}\"")));
    }
    Ok(())
}

#[test]
fn custom_name_title_does_not_disable_the_second_section_warning()
-> Result<(), Box<dyn std::error::Error>> {
    let parsed = parse(
        "= names(1)\n:doctype: manpage\n:manname-title: NOMS\n\n== NAME\n\nnames - purpose\n\n== DESCRIPTION\n\nBody.\n",
        &ParserOptions::default(),
    )?;
    let processor = Processor::new(Options::default(), ParserOptions::builder())?;
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let warning_source = WarningSource::new("manpage");
    processor.write_to(
        parsed.document(),
        &mut output,
        None,
        None,
        &mut Diagnostics::new(&warning_source, &mut warnings),
    )?;
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings.first().map(|warning| warning.message.as_ref()),
        Some("manpage convention: SYNOPSIS should be the second section, got `DESCRIPTION`")
    );
    Ok(())
}
