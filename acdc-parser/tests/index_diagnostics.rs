//! Structured diagnostics for unsupported index section children.

use acdc_parser::{Options, WarningKind, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn index_children_have_structured_warnings_at_each_direct_heading() -> Result<(), Error> {
    let parsed = parse(
        include_str!("../fixtures/tests/index_children.adoc"),
        &Options::default(),
    )?;
    let warnings = parsed
        .warnings()
        .iter()
        .filter(|warning| matches!(warning.kind, WarningKind::NestedSectionInIndex))
        .collect::<Vec<_>>();
    let lines = warnings
        .iter()
        .map(|warning| {
            warning
                .source_location()
                .map(|source| source.location.start.line)
        })
        .collect::<Vec<_>>();
    assert_eq!(lines, [Some(12), Some(18)]);
    assert!(warnings.iter().all(|warning| warning.advice().is_some()));
    Ok(())
}

#[cfg(feature = "setext")]
#[test]
fn index_children_in_setext_sections_have_the_same_diagnostic() -> Result<(), Error> {
    let parsed = parse(
        "= Document\n\n[index]\nIndex\n-----\n\nChild\n~~~~~\n\nDetails.\n",
        &Options::builder().with_setext().build()?,
    )?;
    let warnings = parsed
        .warnings()
        .iter()
        .filter(|warning| matches!(warning.kind, WarningKind::NestedSectionInIndex))
        .collect::<Vec<_>>();
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings
            .first()
            .ok_or("missing warning")?
            .source_location()
            .ok_or("missing location")?
            .location
            .start
            .line,
        7
    );
    Ok(())
}
