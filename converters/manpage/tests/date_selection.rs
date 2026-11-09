use acdc_converters_core::{Converter as _, Diagnostics, Options, WarningSource};
use acdc_converters_manpage::Converter;
use acdc_parser::{CompatibilityMode, Document, Options as ParserOptions, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case(":docdate: 2001-02-03\n:revdate: 2004-05-06\n", "", Some("2001-02-03"))]
#[case(":docdate: 2001-02-03\n:revdate:\n", "", Some("2001-02-03"))]
#[case(":docdate: 2001-02-03\n:revdate!:\n", "", Some("2001-02-03"))]
#[case(":docdate:\n:revdate: 2004-05-06\n", "", Some(""))]
#[case(":docdate!:\n:revdate: 2004-05-06\n", "", None)]
#[case(":docdate: 2001-02-03\n:reproducible:\n", "", None)]
#[case(":docdate: 2001-02-03\n:reproducible: yes\n", "", None)]
#[case(":docdate: 2001-02-03\n:reproducible!:\n", "", Some("2001-02-03"))]
#[case(":docdate: 2001-02-03\n", ":docdate: 2004-05-06\n", Some("2001-02-03"))]
#[case(":docdate: 2001-02-03\n", ":docdate!:\n", Some("2001-02-03"))]
fn date_uses_the_resolved_header_snapshot(
    #[case] header: &str,
    #[case] body: &str,
    #[case] date: Option<&str>,
    #[values(false, true)] embedded: bool,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!(
        "= dates(1)\n:doctype: manpage\n{header}\n== NAME\n\ndates - date controls\n\n{body}\n== SYNOPSIS\n\ndates\n"
    );
    let parsed = parse(
        &source,
        &ParserOptions::builder()
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let before = Document::from(parsed.document());
    let converter = Converter::new(
        Options::builder().embedded(embedded).build(),
        ParserOptions::builder().with_compatibility_mode(mode),
    )?;
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let warning_source = WarningSource::new("manpage");
    converter.write_to(
        parsed.document(),
        &mut output,
        None,
        None,
        &mut Diagnostics::new(&warning_source, &mut warnings),
    )?;
    let output = String::from_utf8(output)?;
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(Document::from(parsed.document()), before);
    assert!(output.contains(".SH \"SYNOPSIS\""));
    if embedded {
        assert!(!output.contains(".TH "));
        assert!(!output.contains("Date:"));
    } else {
        assert!(output.contains(&format!(
            ".TH \"DATES\" \"1\" \"{}\"",
            date.unwrap_or_default()
        )));
        assert_eq!(output.contains("Date:"), date.is_some());
    }
    Ok(())
}
