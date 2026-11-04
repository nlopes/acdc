use acdc_parser::{CompatibilityMode, Options, WarningKind, parse};

type TestError = Box<dyn std::error::Error>;

#[rstest::rstest]
fn unknown_footnote_fallbacks_report_each_recovered_occurrence(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(
        "{}",
        "**{}**",
        "link:manual.html[{}]",
        "pass:m[{}]",
        "pass:c,m[{}]",
        "pass:a,m[{}]"
    )]
    context: &str,
) -> Result<(), TestError> {
    let authored = if context.contains("a,m") {
        r"footnote:{missing-id}[\]"
    } else if context.starts_with("pass:") {
        r"footnote:missing[\]"
    } else {
        "footnote:missing[]"
    };
    let body = format!("{authored} and {authored}");
    let source = format!("Earlier.\n\nα🚀 {}", context.replace("{}", &body));
    let parsed = parse(
        &source,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_attribute("missing-id", "missing")
            .build()?,
    )?;
    assert_eq!(parsed.document().footnotes().count(), 0);
    assert!(parsed.source_recovery().is_some());
    let starts = source
        .match_indices(authored)
        .map(|(start, _)| start)
        .collect::<Vec<_>>();
    assert_eq!(parsed.warnings().len(), starts.len());
    for (warning, start) in parsed.warnings().iter().zip(starts) {
        assert!(matches!(warning.kind, WarningKind::ContentRecovery { .. }));
        let location = &warning
            .source_location()
            .ok_or("missing footnote location")?
            .location;
        assert_eq!(location.absolute_start, start);
        assert_eq!(
            source.get(location.absolute_start..=location.absolute_end),
            Some(authored)
        );
    }
    Ok(())
}

#[rstest::rstest]
fn inline_entrypoint_retains_distinct_warning_locations(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), TestError> {
    let source = "α🚀 **footnote:missing[] and footnote:missing[]**";
    let parsed = acdc_parser::parse_inline(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.warnings().len(), 2);
    for (warning, (start, _)) in parsed
        .warnings()
        .iter()
        .zip(source.match_indices("footnote:missing[]"))
    {
        let location = &warning
            .source_location()
            .ok_or("missing inline location")?
            .location;
        assert_eq!(location.absolute_start, start);
        assert_eq!(
            source.get(location.absolute_start..=location.absolute_end),
            Some("footnote:missing[]")
        );
    }
    Ok(())
}

#[rstest::rstest]
#[case(r"\footnote:missing[]")]
#[case("[subs=none]\nfootnote:missing[]")]
#[case("[subs=\"-macros\"]\nfootnote:missing[]")]
fn literal_footnotes_do_not_report_recovery(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] source: &str,
) -> Result<(), TestError> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.warnings(), []);
    assert!(parsed.source_recovery().is_none());
    Ok(())
}
#[rstest::rstest]
fn earlier_mapped_warnings_do_not_hide_later_substring_warnings(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), TestError> {
    let source = "footnote:missing[]\n\nfootnote:missing[]";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.warnings().len(), 2);
    for (warning, (start, _)) in parsed
        .warnings()
        .iter()
        .zip(source.match_indices("footnote:missing[]"))
    {
        assert_eq!(
            warning
                .source_location()
                .ok_or("missing location")?
                .location
                .absolute_start,
            start
        );
    }
    Ok(())
}
#[rstest::rstest]
fn bibliography_label_warnings_use_the_label_source(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), TestError> {
    let source = "Earlier.\n\n* [[[book,Before footnote:missing[] after]]]";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [warning] = parsed.warnings() else {
        return Err(format!("expected one label warning: {:?}", parsed.warnings()).into());
    };
    let location = &warning
        .source_location()
        .ok_or("missing bibliography location")?
        .location;
    assert_eq!(
        location.absolute_start,
        source.find("footnote:").ok_or("authored reference")?
    );
    assert_eq!(
        source.get(location.absolute_start..=location.absolute_end),
        Some("footnote:missing[]")
    );
    Ok(())
}
