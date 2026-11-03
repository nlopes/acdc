use acdc_parser::{CompatibilityMode, Options, WarningKind, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("pass:q,c[*Éva*]")]
#[case("stem:quotes[α *x*]")]
#[case("{counter:items}")]
#[case("{counter2:items:4}")]
fn preprocessor_warnings_use_inclusive_source_spans(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("{}", "**{}**", "footnote:n[Before {} after]", "link:manual.html[{}]")] context: &str,
    #[case] body: &str,
) -> Result<(), Error> {
    let source = format!("Earlier.\n\nα🚀 {} tail.", context.replace("{}", body));
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [warning] = parsed.warnings() else {
        return Err(format!("expected one warning: {:?}", parsed.warnings()).into());
    };
    let location = &warning
        .source_location()
        .ok_or("missing location")?
        .location;
    let start = source.find(body).ok_or("missing authored body")?;
    assert_eq!(location.absolute_start, start, "{source}");
    assert_eq!(location.absolute_end, start + body.len() - 1, "{source}");
    assert_eq!(
        source.get(location.absolute_start..=location.absolute_end),
        Some(body)
    );
    assert!(matches!(
        warning.kind,
        WarningKind::UnsupportedCounter { .. } | WarningKind::ContentRecovery { .. }
    ));
    assert!(parsed.source_recovery().is_some());
    Ok(())
}

#[rstest::rstest]
#[case(r"\{counter:items}")]
#[case(r"\\{counter:items}")]
#[case(r"\\\{counter:items}")]
#[case(r"{counter:items\}")]
#[case(r"\{counter:items\}")]
#[case(r"{counter2:items:4\}")]
#[case("pass:[{counter:items}]")]
#[case("[subs=none]\n{counter:items} pass:q,c[*Literal*] stem:quotes[x]")]
fn deliberately_literal_controls_are_not_recoveries(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] source: &str,
) -> Result<(), Error> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.warnings(), []);
    assert!(parsed.source_recovery().is_none());
    Ok(())
}

#[rstest::rstest]
fn taking_warnings_keeps_counter_recovery_available(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let mut parsed = parse(
        "Before {counter:items} after.",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.take_warnings().len(), 1);
    assert_eq!(parsed.warnings(), []);
    assert!(matches!(
        parsed.source_recovery().map(|warning| &warning.kind),
        Some(WarningKind::UnsupportedCounter { .. })
    ));
    Ok(())
}
