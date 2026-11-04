//! Heading diagnostics preserve text classification and structural exclusions.

use acdc_lint::{LintGroup, LintId, LintLevel, LintOptions, LintOverride, LintSelector, Lintable};
use acdc_parser::CompatibilityMode;

type Error = Box<dyn std::error::Error>;

#[test]
fn capitalization_keeps_monospace_classification_and_skips_table_headings() -> Result<(), Error> {
    let source = "= document\nAuthor Name\nv1.0, 2026-01-01\n\n== `section`\n\n* Item\n+\n--\n[discrete]\n=== _discrete_\n--\n\n[cols=a]\n|===\n|[discrete]\n=== hidden\n|===\n";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = LintOptions::new(vec![
            LintOverride::new(LintLevel::Allow, LintSelector::Group(LintGroup::All)),
            LintOverride::new(
                LintLevel::Deny,
                LintSelector::Lint(LintId::SectionTitleCapitalization),
            ),
            LintOverride::new(
                LintLevel::Warn,
                LintSelector::Lint(LintId::SectionTitleCapitalizationMonospace),
            ),
        ])
        .with_compatibility_mode(mode);
        let report = source.lint(&options)?;
        let actual = report
            .diagnostics()
            .iter()
            .map(|diagnostic| {
                let location = diagnostic.location().ok_or("missing heading location")?;
                Ok((
                    diagnostic.lint(),
                    diagnostic.level(),
                    location.location.start.line,
                ))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        assert_eq!(
            actual,
            [
                (LintId::SectionTitleCapitalization, LintLevel::Deny, 1),
                (
                    LintId::SectionTitleCapitalizationMonospace,
                    LintLevel::Warn,
                    5
                ),
                (LintId::SectionTitleCapitalization, LintLevel::Deny, 11),
            ]
        );
    }
    Ok(())
}
