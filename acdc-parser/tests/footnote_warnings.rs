use std::path::Path;

use acdc_parser::{Options, Warning, WarningKind, parse, parse_file};

type Error = Box<dyn std::error::Error>;

fn conflicts(warnings: &[Warning]) -> Vec<&Warning> {
    warnings
        .iter()
        .filter(|warning| matches!(warning.kind, WarningKind::ConflictingFootnote { .. }))
        .collect()
}

#[test]
fn footnote_conflicts_keep_passthrough_boundaries_and_expanded_bodies() -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/footnote_conflicting_passthroughs.adoc");
    let parsed = parse(source, &Options::default())?;
    let ids = conflicts(parsed.warnings())
        .into_iter()
        .filter_map(|warning| {
            if let WarningKind::ConflictingFootnote { id, .. } = &warning.kind {
                Some(id.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        ["markup", "attribute", "macro"],
        "{:?}",
        parsed.warnings()
    );
    Ok(())
}

#[test]
fn conflicting_footnote_definitions_warn_at_each_changed_body() -> Result<(), Error> {
    // Parser snapshots omit diagnostics and the document's footnote catalog.
    let source = include_str!("../fixtures/tests/footnote_conflicting_definitions.adoc");
    let parsed = parse(source, &Options::default())?;
    let warnings = conflicts(parsed.warnings());
    assert_eq!(warnings.len(), 4, "{:?}", parsed.warnings());
    let expected = [
        (
            "shared",
            "footnote:shared[Original text.]",
            "footnote:shared[Replacement with ((Bananas)).]",
        ),
        (
            "shared",
            "footnote:shared[Original text.]",
            "footnote:shared[Another replacement.]",
        ),
        (
            "expanded",
            "footnote:expanded[Text {name}.]",
            "footnote:expanded[Text {name}.]",
        ),
        (
            "pass",
            "footnote:pass[Value +literal+.]",
            "footnote:pass[Value +changed+.]",
        ),
    ];
    for (warning, (expected_id, original, repeated)) in warnings.into_iter().zip(expected) {
        let WarningKind::ConflictingFootnote { id, first } = &warning.kind else {
            return Err("unexpected warning".into());
        };
        assert_eq!(id, expected_id);
        assert_eq!(
            source.get(first.location.absolute_start..=first.location.absolute_end),
            Some(original)
        );
        let location = &warning
            .source_location()
            .ok_or("missing conflicting location")?
            .location;
        assert_eq!(
            source.get(location.absolute_start..=location.absolute_end),
            Some(repeated)
        );
        assert!(location.absolute_start > first.location.absolute_start);
        assert!(warning.to_string().contains("keeping the first definition"));
        assert!(warning.advice().is_some());
    }
    assert_eq!(parsed.document().footnotes.len(), 5);
    assert_eq!(
        parsed
            .document()
            .footnotes
            .iter()
            .map(|note| note.number)
            .collect::<Vec<_>>(),
        [1, 2, 3, 4, 5]
    );
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn footnote_conflict_comparison_respects_macro_registration_order() -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/subs_footnote_conflicting_definitions.adoc");
    let parsed = parse(source, &Options::default())?;
    let warnings = conflicts(parsed.warnings());
    let [warning] = warnings.as_slice() else {
        return Err(format!("expected one conflict: {:?}", parsed.warnings()).into());
    };
    assert!(matches!(&warning.kind, WarningKind::ConflictingFootnote { id, .. } if id == "early"));
    assert_eq!(parsed.document().footnotes.len(), 2);
    Ok(())
}

#[test]
fn conflicting_footnotes_report_both_included_files() -> Result<(), Error> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/preprocessor");
    let parsed = parse_file(
        directory.join("footnote_conflict_root.adoc"),
        &Options::builder()
            .with_safe_mode(acdc_parser::SafeMode::Unsafe)
            .build()?,
    )?;
    let warnings = conflicts(parsed.warnings());
    let [warning] = warnings.as_slice() else {
        return Err(format!("expected one conflict: {:?}", parsed.warnings()).into());
    };
    let WarningKind::ConflictingFootnote { id, first } = &warning.kind else {
        return Err("unexpected warning".into());
    };
    assert_eq!(id, "shared");
    let location = warning
        .source_location()
        .ok_or("missing conflicting location")?;
    assert_eq!(
        first.file.as_deref(),
        Some(directory.join("footnote_conflict_first.adoc").as_path())
    );
    assert_eq!(
        location.file.as_deref(),
        Some(directory.join("footnote_conflict_second.adoc").as_path())
    );
    assert_eq!(first.location.start.line, 1);
    assert_eq!(location.location.start.line, 3);
    assert_eq!(location.location.start.column, 36);
    Ok(())
}
