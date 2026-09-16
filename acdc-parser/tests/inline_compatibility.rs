use acdc_parser::{Options, parse};

type Error = Box<dyn std::error::Error>;

#[cfg(feature = "pre-spec-subs")]
#[test]
fn disabled_attribute_substitutions_do_not_warn_about_counters() -> Result<(), Error> {
    for subs in ["quotes", "none", "normal,-attributes"] {
        let source = format!(
            "[subs=\"{subs}\"]\nLiteral {{counter:seq}}, {{counter:seq:4}}, and {{counter2:seq}} with *bold*.\n"
        );
        let parsed = parse(&source, &Options::default())?;
        assert!(
            parsed
                .warnings()
                .iter()
                .all(|warning| !warning.kind.to_string().contains("Counters")),
            "{:?}",
            parsed.warnings()
        );
    }
    Ok(())
}
