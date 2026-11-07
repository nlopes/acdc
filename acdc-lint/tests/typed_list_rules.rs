//! List diagnostics retain parent depth and source order through attached blocks.

use acdc_lint::{LintGroup, LintId, LintLevel, LintOptions, LintOverride, LintSelector, Lintable};
use acdc_parser::CompatibilityMode;

type Error = Box<dyn std::error::Error>;

#[test]
fn nested_markers_keep_list_depth_through_containers_and_cells() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = LintOptions::new(vec![
            LintOverride::new(LintLevel::Allow, LintSelector::Group(LintGroup::All)),
            LintOverride::new(
                LintLevel::Deny,
                LintSelector::Lint(LintId::NestedUnorderedListMarker),
            ),
        ])
        .with_compatibility_mode(mode);
        for parent in [
            "* Parent λ",
            ". Parent λ",
            "Term:: Parent λ",
            "[source,sh]\n----\necho source <1>\n----\n<1> Parent λ",
        ] {
            let prefix = format!("= Document\n\n{parent}\n+\n");
            for table in [false, true] {
                let (wrapper, closing, column) = if table {
                    ("[cols=a]\n|===\n|", "|===\n", 2)
                } else {
                    ("--\n", "--\n", 1)
                };
                let source = format!("{prefix}{wrapper}- Child\n+\n- Grandchild\n{closing}");
                let first_line = u32::try_from(prefix.lines().count() + wrapper.lines().count())?
                    + u32::from(!table);
                let report = source.as_str().lint(&options)?;
                let actual = report
                    .diagnostics()
                    .iter()
                    .map(|diagnostic| {
                        assert_eq!(diagnostic.lint(), LintId::NestedUnorderedListMarker);
                        assert_eq!(diagnostic.level(), LintLevel::Deny);
                        let location = diagnostic.location().ok_or("missing item location")?;
                        Ok((location.location.start.line, location.location.start.column))
                    })
                    .collect::<Result<Vec<_>, Error>>()?;
                assert_eq!(
                    actual,
                    [(first_line, column), (first_line + 2, 1)],
                    "{mode:?}: {source}"
                );
            }
        }
    }
    Ok(())
}
