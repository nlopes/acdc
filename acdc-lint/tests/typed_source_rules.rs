//! Source diagnostics retain their order and ranges through typed tree traversal.

use acdc_lint::{LintGroup, LintId, LintLevel, LintOptions, LintOverride, LintSelector, Lintable};
use acdc_parser::CompatibilityMode;

type Error = Box<dyn std::error::Error>;

fn options(mode: CompatibilityMode, rule: LintId) -> LintOptions {
    LintOptions::new(vec![
        LintOverride::new(LintLevel::Allow, LintSelector::Group(LintGroup::All)),
        LintOverride::new(LintLevel::Deny, LintSelector::Lint(rule)),
    ])
    .with_compatibility_mode(mode)
}

#[test]
fn accepted_url_assignments_keep_order_and_locations() -> Result<(), Error> {
    let source = "= Document\nAuthor Name\nv1.0, 2026-01-01\n:homepage: https://header.example.test\n\n:body: https://body.example.test\n\n* Item\n+\n--\n:list: https://list.example.test\n\nText.\n--\n\n[cols=a]\n|===\n|:cell: https://cell.example.test\n\nCell.\n|===\n";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let report = source.lint(&options(mode, LintId::AttributeUrlPrefix))?;
        let actual = report
            .diagnostics()
            .iter()
            .map(|diagnostic| {
                assert_eq!(diagnostic.lint(), LintId::AttributeUrlPrefix);
                assert_eq!(diagnostic.level(), LintLevel::Deny);
                let location = diagnostic.location().ok_or("missing source location")?;
                assert!(location.file.is_none());
                Ok((
                    diagnostic.message(),
                    location.location.start.line,
                    location.location.start.column,
                ))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let expected = [
            ("homepage", 4, 1),
            ("body", 6, 1),
            ("list", 11, 1),
            ("cell", 18, 2),
        ]
        .map(|(name, line, column)| {
            (
                format!("URL-valued attribute `{name}` should use a url- or uri- prefix"),
                line,
                column,
            )
        });
        assert_eq!(
            actual,
            expected
                .iter()
                .map(|(message, line, column)| (message.as_str(), *line, *column))
                .collect::<Vec<_>>()
        );
        assert!(report.has_errors());
    }
    Ok(())
}

#[test]
fn source_blank_lines_stay_exempt_in_nested_blocks() -> Result<(), Error> {
    let listing = "[source,sh]\n----\necho first\n\n\necho last\n----\n";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for wrapper in ["open", "list", "table"] {
            let mut source = match wrapper {
                "open" => format!("--\n{listing}--\n"),
                "list" => format!("* Item\n+\n{listing}"),
                _ => format!("[cols=a]\n|===\n|{listing}|===\n"),
            };
            let expected_line = u32::try_from(source.lines().count())? + 4;
            source.push_str("\nOutside\n\n\nEnd.\n");
            let report = source
                .as_str()
                .lint(&options(mode, LintId::ExcessiveBlankLines))?;
            drop(source);
            let [diagnostic] = report.diagnostics() else {
                return Err(
                    format!("expected one outside blank-line diagnostic: {report:?}").into(),
                );
            };
            assert_eq!(diagnostic.lint(), LintId::ExcessiveBlankLines);
            assert_eq!(diagnostic.level(), LintLevel::Deny);
            assert_eq!(
                diagnostic
                    .location()
                    .ok_or("missing source location")?
                    .location
                    .start
                    .line,
                expected_line
            );
        }
    }
    Ok(())
}
