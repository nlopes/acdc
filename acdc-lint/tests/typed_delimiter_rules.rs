//! Delimiter advice preserves collision checks and authored opening ranges.

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
fn shortening_uses_each_delimiter_and_its_opening_range() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for (opening, body, closing, minimum) in [
            ("-------", "Text.", "-------", Some(4)),
            ("-------", "----", "-------", Some(5)),
            (".......", "Text.", ".......", Some(4)),
            ("=======", "Text.", "=======", Some(4)),
            ("*******", "Text.", "*******", Some(4)),
            ("_______", "Text.", "_______", Some(4)),
            ("+++++++", "Text.", "+++++++", Some(4)),
            ("///////", "Text.", "///////", Some(4)),
            ("|======", "|Cell.", "|======", Some(4)),
            ("--", "Text.", "--", None),
            ("```rust", "Text.", "```", None),
            ("-------", "Text.", "", Some(4)),
        ] {
            let source = format!(
                "= Document\n\n.Block title\n[role=sample]\n{opening}\n{body}\n{closing}\n"
            );
            let report = source
                .as_str()
                .lint(&options(mode, LintId::DelimitedBlockMinimalDelimiter))?;
            if let Some(minimum) = minimum {
                let [diagnostic] = report.diagnostics() else {
                    return Err(
                        format!("expected one diagnostic for {opening:?}: {report:?}").into(),
                    );
                };
                assert!(
                    diagnostic
                        .message()
                        .ends_with(&format!("can be shortened to {minimum} characters"))
                );
                assert_eq!(diagnostic.level(), LintLevel::Deny);
                let location = diagnostic.location().ok_or("missing opening range")?;
                assert_eq!(
                    (location.location.start.line, location.location.start.column),
                    (5, 1)
                );
            } else {
                assert!(report.is_empty(), "{opening:?}: {report:?}");
            }
        }
    }
    Ok(())
}

#[test]
fn multiple_titles_respect_the_exact_book_value() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for (doctype, count) in [("book", 0), ("article", 1), ("\"book\"", 1)] {
            let source = format!("= First\n:doctype: {doctype}\n\n= Second\n");
            let report = source
                .as_str()
                .lint(&options(mode, LintId::MultipleDocumentTitle))?;
            assert_eq!(
                report.diagnostics().len(),
                count,
                "{mode:?}, {doctype}: {report:?}"
            );
        }
    }
    Ok(())
}
